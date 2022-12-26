use std::cmp::min;
use std::convert::TryFrom;
use std::io;

use anyhow::ensure;
use anyhow::Error;
use positioned_io::ReadAt;

use crate::{
    assumption_failed, map_lib_error_to_io, read_le16, read_le32, Crypto, InnerReader,
    MetadataCrypto,
};

#[derive(Debug)]
struct Extent {
    /// The docs call this 'block' (like everything else). I've invented a different name.
    part: u32,
    start: u64,
    len: u16,
}

pub struct TreeReader<'a, R: ReadAt, C: Crypto, M: MetadataCrypto> {
    inner: &'a InnerReader<R, M>,
    pos: u64,
    len: u64,
    block_size: u32,
    extents: Vec<Extent>,
    encryption_context: Option<&'a Vec<u8>>,
    crypto: &'a C,
}

impl<'a, R: ReadAt, C: Crypto, M: MetadataCrypto> TreeReader<'a, R, C, M> {
    pub fn new(
        inner: &'a InnerReader<R, M>,
        block_size: u32,
        size: u64,
        core: [u8; crate::INODE_CORE_SIZE],
        checksum_prefix: Option<u32>,
        encryption_context: Option<&'a Vec<u8>>,
        crypto: &'a C,
    ) -> Result<TreeReader<'a, R, C, M>, Error> {
        let extents = load_extent_tree(
            &mut |block| crate::load_disc_bytes(inner, block_size, block),
            core,
            checksum_prefix,
        )?;
        Ok(TreeReader::create(
            inner,
            block_size,
            size,
            extents,
            encryption_context,
            crypto,
        ))
    }

    fn create(
        inner: &'a InnerReader<R, M>,
        block_size: u32,
        size: u64,
        extents: Vec<Extent>,
        encryption_context: Option<&'a Vec<u8>>,
        crypto: &'a C,
    ) -> TreeReader<'a, R, C, M> {
        TreeReader {
            pos: 0,
            len: size,
            inner,
            extents,
            block_size,
            encryption_context,
            crypto,
        }
    }

    pub fn ref_inner(self) -> &'a R {
        &self.inner.inner
    }
}

enum FoundPart<'a> {
    Actual(&'a Extent),
    Sparse(u32),
}

fn find_part(part: u32, extents: &[Extent]) -> FoundPart {
    for extent in extents {
        if part < extent.part {
            // we've gone past it
            return FoundPart::Sparse(extent.part - part);
        }

        if part >= extent.part && part < extent.part + u32::from(extent.len) {
            // we're inside it
            return FoundPart::Actual(extent);
        }
    }

    FoundPart::Sparse(std::u32::MAX)
}

impl<'a, R: ReadAt, C: Crypto, M: MetadataCrypto> io::Read for TreeReader<'a, R, C, M> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let block_size = u64::from(self.block_size);

        let wanted_block = u32::try_from(self.pos / block_size).map_err(map_lib_error_to_io)?;
        let read_of_this_block = self.pos % block_size;

        match find_part(wanted_block, &self.extents) {
            FoundPart::Actual(extent) => {
                let bytes_through_extent =
                    (block_size * u64::from(wanted_block - extent.part)) + read_of_this_block;
                let remaining_bytes_in_extent =
                    (u64::from(extent.len) * block_size) - bytes_through_extent;
                let to_read = min(remaining_bytes_in_extent, buf.len() as u64) as usize;
                let to_read = min(to_read as u64, self.len - self.pos) as usize;
                let offset = extent.start * block_size + bytes_through_extent;

                let read = if let Some(context) = self.encryption_context {
                    let mut read_offset = 0;
                    let chunk_size = block_size as usize;

                    while read_offset < to_read {
                        let mut block_buffer = vec![0u8; chunk_size];
                        self.inner.read_at_without_decrypt(
                            offset + read_offset as u64,
                            &mut block_buffer[0..chunk_size],
                        )?;

                        self.crypto
                            .decrypt_page(
                                context,
                                &mut block_buffer[0..chunk_size],
                                offset + read_offset as u64,
                            )
                            .map_err(map_lib_error_to_io)?;

                        let expected_size = min(to_read - read_offset, chunk_size);
                        buf[read_offset..read_offset + expected_size]
                            .copy_from_slice(&block_buffer[..expected_size]);

                        read_offset += chunk_size;
                    }

                    to_read
                } else {
                    self.inner.read_at(offset, &mut buf[0..to_read])?
                };

                self.pos += u64::try_from(read).map_err(map_lib_error_to_io)?;
                Ok(read)
            }
            FoundPart::Sparse(max) => {
                let max_bytes = u64::from(max) * block_size;
                let read = min(max_bytes, buf.len() as u64) as usize;
                let read = min(read as u64, self.len - self.pos) as usize;
                zero(&mut buf[0..read]);
                self.pos += u64::try_from(read).map_err(map_lib_error_to_io)?;
                Ok(read)
            }
        }
    }
}

impl<'a, R: ReadAt, C: Crypto, M: MetadataCrypto> io::Seek for TreeReader<'a, R, C, M> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        match pos {
            io::SeekFrom::Start(set) => self.pos = set,
            io::SeekFrom::Current(diff) => self.pos = (self.pos as i64 + diff) as u64,
            io::SeekFrom::End(set) => {
                assert!(set >= 0);
                self.pos = self.len - u64::try_from(set).map_err(map_lib_error_to_io)?;
            }
        }

        assert!(self.pos <= self.len);

        Ok(self.pos)
    }
}

fn add_found_extents<F>(
    load_block: &mut F,
    data: &[u8],
    expected_depth: u16,
    extents: &mut Vec<Extent>,
    checksum_prefix_op: Option<u32>,
    first_level: bool,
) -> Result<(), Error>
where
    F: FnMut(u64) -> Result<Vec<u8>, Error>,
{
    ensure!(
        0x0a == data[0] && 0xf3 == data[1],
        assumption_failed("invalid extent magic")
    );

    let extent_entries = read_le16(&data[2..]);
    // 4..: max; doesn't seem to be useful during read
    let depth = read_le16(&data[6..]);
    // 8..: generation, not used in standard ext4

    ensure!(
        expected_depth == depth,
        assumption_failed(format!("depth incorrect: {} != {}", expected_depth, depth))
    );

    if let (Some(checksum_prefix), false) = (checksum_prefix_op, first_level) {
        let end_of_entries = data.len() - 4;
        let on_disc = read_le32(&data[end_of_entries..(end_of_entries + 4)]);
        let computed = crate::parse::ext4_style_crc32c_le(checksum_prefix, &data[..end_of_entries]);

        ensure!(
            computed == on_disc,
            assumption_failed(format!(
                "extent checksum mismatch: {:08x} != {:08x} @ {}",
                on_disc,
                computed,
                data.len()
            ),)
        );
    }

    if 0 == depth {
        for en in 0..extent_entries {
            let raw_extent = &data[12 + usize::from(en) * 12..];
            let ee_block = read_le32(raw_extent);
            let ee_len = read_le16(&raw_extent[4..]);
            let ee_start_hi = read_le16(&raw_extent[6..]);
            let ee_start_lo = read_le32(&raw_extent[8..]);
            let ee_start = u64::from(ee_start_lo) + 0x1000 * u64::from(ee_start_hi);

            extents.push(Extent {
                part: ee_block,
                start: ee_start,
                len: ee_len,
            });
        }

        return Ok(());
    }

    for en in 0..extent_entries {
        let extent_idx = &data[12 + usize::from(en) * 12..];
        //            let ei_block = as_u32(extent_idx);
        let ei_leaf_lo = read_le32(&extent_idx[4..]);
        let ei_leaf_hi = read_le16(&extent_idx[8..]);
        let ee_leaf: u64 = u64::from(ei_leaf_lo) + (u64::from(ei_leaf_hi) << 32);
        let data = load_block(ee_leaf)?;
        add_found_extents(
            load_block,
            &data,
            depth - 1,
            extents,
            checksum_prefix_op,
            false,
        )?;
    }

    Ok(())
}

fn load_extent_tree<F>(
    load_block: &mut F,
    core: [u8; crate::INODE_CORE_SIZE],
    checksum_prefix: Option<u32>,
) -> Result<Vec<Extent>, Error>
where
    F: FnMut(u64) -> Result<Vec<u8>, Error>,
{
    ensure!(
        0x0a == core[0] && 0xf3 == core[1],
        assumption_failed("invalid extent magic")
    );

    let extent_entries = read_le16(&core[2..]);
    // 4..: max; doesn't seem to be useful during read
    let depth = read_le16(&core[6..]);

    ensure!(
        depth <= 5,
        assumption_failed(format!("initial depth too high: {}", depth))
    );

    let mut extents = Vec::with_capacity(usize::from(extent_entries) + usize::from(depth) * 200);

    add_found_extents(
        load_block,
        &core,
        depth,
        &mut extents,
        checksum_prefix,
        true,
    )?;

    extents.sort_by_key(|e| e.part);

    Ok(extents)
}

fn zero(buf: &mut [u8]) {
    unsafe { std::ptr::write_bytes(buf.as_mut_ptr(), 0u8, buf.len()) }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;
    use std::io::Read;

    use crate::extents::Extent;
    use crate::extents::TreeReader;
    use crate::{InnerReader, NoneCrypto};

    #[test]
    fn simple_tree() {
        let size = 4 + 4 * 2;
        let crypto = NoneCrypto {};
        let metadata_crypto = NoneCrypto {};
        let data = InnerReader::new((0..255u8).collect::<Vec<u8>>(), metadata_crypto);
        let mut reader = TreeReader::create(
            &data,
            4,
            u64::try_from(size).expect("infallible u64 conversion"),
            vec![
                Extent {
                    part: 0,
                    start: 10,
                    len: 1,
                },
                Extent {
                    part: 1,
                    start: 20,
                    len: 2,
                },
            ],
            None,
            &crypto,
        );

        let mut res = Vec::new();
        assert_eq!(size, reader.read_to_end(&mut res).unwrap());

        assert_eq!(vec![40, 41, 42, 43, 80, 81, 82, 83, 84, 85, 86, 87], res);
    }

    #[test]
    fn zero_buf() {
        let mut buf = [7u8; 5];
        assert_eq!(7, buf[0]);
        crate::extents::zero(&mut buf);
        for i in &buf {
            assert_eq!(0, *i);
        }
    }
}
