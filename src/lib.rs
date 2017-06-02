#[macro_use] extern crate bitflags;
extern crate byteorder;

use std::io;

use byteorder::{ReadBytesExt, LittleEndian, BigEndian};

use std::io::Read;
use std::io::Seek;

pub mod mbr;

const EXT4_SUPER_MAGIC: u16 = 0xEF53;

const EXT4_BLOCK_GROUP_INODES_UNUSED: u16 = 0b1;
const EXT4_BLOCK_GROUP_BLOCKS_UNUSED: u16 = 0b10;

bitflags! {
    struct IncompatibleFeature: u32 {
       const INCOMPAT_COMPRESSION = 0x0001;
       const INCOMPAT_FILETYPE    = 0x0002;
       const INCOMPAT_RECOVER     = 0x0004; /* Needs recovery */
       const INCOMPAT_JOURNAL_DEV = 0x0008; /* Journal device */
       const INCOMPAT_META_BG     = 0x0010;
       const INCOMPAT_EXTENTS     = 0x0040; /* extents support */
       const INCOMPAT_64BIT       = 0x0080;
       const INCOMPAT_MMP         = 0x0100;
       const INCOMPAT_FLEX_BG     = 0x0200;
       const INCOMPAT_EA_INODE    = 0x0400; /* EA in inode */
       const INCOMPAT_DIRDATA     = 0x1000; /* data in dirent */
       const INCOMPAT_CSUM_SEED   = 0x2000;
       const INCOMPAT_LARGEDIR    = 0x4000; /* >2GB or 3-lvl htree */
       const INCOMPAT_INLINE_DATA = 0x8000; /* data in inode */
       const INCOMPAT_ENCRYPT     = 0x10000;
    }
}

bitflags! {
    struct InodeFlags: u32 {
        const INODE_SECRM        = 0x00000001; /* Secure deletion */
        const INODE_UNRM         = 0x00000002; /* Undelete */
        const INODE_COMPR        = 0x00000004; /* Compress file */
        const INODE_SYNC         = 0x00000008; /* Synchronous updates */
        const INODE_IMMUTABLE    = 0x00000010; /* Immutable file */
        const INODE_APPEND       = 0x00000020; /* writes to file may only append */
        const INODE_NODUMP       = 0x00000040; /* do not dump file */
        const INODE_NOATIME      = 0x00000080; /* do not update atime */
        const INODE_DIRTY        = 0x00000100; /* reserved for compression */
        const INODE_COMPRBLK     = 0x00000200; /* One or more compressed clusters */
        const INODE_NOCOMPR      = 0x00000400; /* Don't compress */
        const INODE_ENCRYPT      = 0x00000800; /* encrypted file */
        const INODE_INDEX        = 0x00001000; /* hash-indexed directory */
        const INODE_IMAGIC       = 0x00002000; /* AFS directory */
        const INODE_JOURNAL_DATA = 0x00004000; /* file data should be journaled */
        const INODE_NOTAIL       = 0x00008000; /* file tail should not be merged */
        const INODE_DIRSYNC      = 0x00010000; /* dirsync behaviour (directories only) */
        const INODE_TOPDIR       = 0x00020000; /* Top of directory hierarchies*/
        const INODE_HUGE_FILE    = 0x00040000; /* Set to each huge file */
        const INODE_EXTENTS      = 0x00080000; /* Inode uses extents */
        const INODE_EA_INODE     = 0x00200000; /* Inode used for large EA */
        const INODE_EOFBLOCKS    = 0x00400000; /* Blocks allocated beyond EOF */
        const INODE_INLINE_DATA  = 0x10000000; /* Inode has inline data. */
        const INODE_PROJINHERIT  = 0x20000000; /* Create with parents projid */
        const INODE_RESERVED     = 0x80000000; /* reserved for ext4 lib */
    }
}

#[derive(Debug, PartialEq)]
pub enum FileType {
    RegularFile,     // S_IFREG (Regular file)
    SymbolicLink,    // S_IFLNK (Symbolic link)
    CharacterDevice, // S_IFCHR (Character device)
    BlockDevice,     // S_IFBLK (Block device)
    Directory,       // S_IFDIR (Directory)
    Fifo,            // S_IFIFO (FIFO)
    Socket,          // S_IFSOCK (Socket)
}

#[derive(Debug)]
pub enum Enhanced {
    RegularFile,
    SymbolicLink(String),
    CharacterDevice(u16, u32),
    BlockDevice(u16, u32),
    Directory(Vec<DirEntry>),
    Fifo,
    Socket,
}

impl FileType {
    fn from_mode(mode: u16) -> Option<FileType> {
        match mode >> 12 {
            0x1 => Some(FileType::Fifo),
            0x2 => Some(FileType::CharacterDevice),
            0x4 => Some(FileType::Directory),
            0x6 => Some(FileType::BlockDevice),
            0x8 => Some(FileType::RegularFile),
            0xA => Some(FileType::SymbolicLink),
            0xC => Some(FileType::Socket),
            _ => None,
        }
    }

    fn from_dir_hint(hint: u8) -> Option<FileType> {
        match hint {
            1 => Some(FileType::RegularFile),
            2 => Some(FileType::Directory),
            3 => Some(FileType::CharacterDevice),
            4 => Some(FileType::BlockDevice),
            5 => Some(FileType::Fifo),
            6 => Some(FileType::Socket),
            7 => Some(FileType::SymbolicLink),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct DirEntry {
    pub inode: u32,
    pub file_type: FileType,
    pub name: String,
}

#[derive(Debug)]
struct Extent {
    block: u32,
    start: u64,
    len: u16,
}

pub struct TreeReader<R> {
    inner: R,
    pos: u64,
    block_size: u32,
    extents: Vec<Extent>,
    sparse_bytes: Option<u64>,
}

#[derive(Debug)]
pub struct Stat {
    pub extracted_type: FileType,
    pub file_mode: u16,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub atime: Time,
    pub ctime: Time,
    pub mtime: Time,
    pub btime: Option<Time>,
    pub link_count: u16,
}

pub struct Inode {
    pub stat: Stat,
    pub number: u32,
    flags: InodeFlags,
    block: [u8; 4 * 15],
}

#[derive(Debug)]
struct BlockGroup {
    inode_table_block: u64,
    inodes: u32,
}

#[derive(Debug)]
pub struct SuperBlock {
    block_size: u32,
    inode_size: u16,
    inodes_per_group: u32,
    groups: Vec<BlockGroup>,
}

#[derive(Debug)]
pub struct Time {
    pub epoch_secs: u32,
    pub nanos: Option<u32>,
}

impl SuperBlock {
    pub fn load<R>(inner: &mut R) -> io::Result<SuperBlock>
    where R: io::Read + io::Seek
    {
        inner.seek(io::SeekFrom::Start(1024))?;

        // <a cut -c 9- | fgrep ' s_' | fgrep -v ERR_ | while read ty nam comment; do printf "let %s =\n  inner.read_%s::<LittleEndian>()?; %s\n" $(echo $nam | tr -d ';') $(echo $ty | sed 's/__le/u/; s/__//') $comment; done
//        let s_inodes_count =
            inner.read_u32::<LittleEndian>()?; /* Inodes count */
        let s_blocks_count_lo =
            inner.read_u32::<LittleEndian>()?; /* Blocks count */
//        let s_r_blocks_count_lo =
            inner.read_u32::<LittleEndian>()?; /* Reserved blocks count */
//        let s_free_blocks_count_lo =
            inner.read_u32::<LittleEndian>()?; /* Free blocks count */
//        let s_free_inodes_count =
            inner.read_u32::<LittleEndian>()?; /* Free inodes count */
        let s_first_data_block =
            inner.read_u32::<LittleEndian>()?; /* First Data Block */
        let s_log_block_size =
            inner.read_u32::<LittleEndian>()?; /* Block size */
//        let s_log_cluster_size =
            inner.read_u32::<LittleEndian>()?; /* Allocation cluster size */
        let s_blocks_per_group =
            inner.read_u32::<LittleEndian>()?; /* # Blocks per group */
//        let s_clusters_per_group =
            inner.read_u32::<LittleEndian>()?; /* # Clusters per group */
        let s_inodes_per_group =
            inner.read_u32::<LittleEndian>()?; /* # Inodes per group */
//        let s_mtime =
            inner.read_u32::<LittleEndian>()?; /* Mount time */
//        let s_wtime =
            inner.read_u32::<LittleEndian>()?; /* Write time */
//        let s_mnt_count =
            inner.read_u16::<LittleEndian>()?; /* Mount count */
//        let s_max_mnt_count =
            inner.read_u16::<LittleEndian>()?; /* Maximal mount count */
        let s_magic =
            inner.read_u16::<LittleEndian>()?; /* Magic signature */
        let s_state =
            inner.read_u16::<LittleEndian>()?; /* File system state */
//        let s_errors =
            inner.read_u16::<LittleEndian>()?; /* Behaviour when detecting errors */
//        let s_minor_rev_level =
            inner.read_u16::<LittleEndian>()?; /* minor revision level */
//        let s_lastcheck =
            inner.read_u32::<LittleEndian>()?; /* time of last check */
//        let s_checkinterval =
            inner.read_u32::<LittleEndian>()?; /* max. time between checks */
        let s_creator_os =
            inner.read_u32::<LittleEndian>()?; /* OS */
        let s_rev_level =
            inner.read_u32::<LittleEndian>()?; /* Revision level */
//        let s_def_resuid =
            inner.read_u16::<LittleEndian>()?; /* Default uid for reserved blocks */
//        let s_def_resgid =
            inner.read_u16::<LittleEndian>()?; /* Default gid for reserved blocks */
//        let s_first_ino =
            inner.read_u32::<LittleEndian>()?; /* First non-reserved inode */
        let s_inode_size =
            inner.read_u16::<LittleEndian>()?; /* size of inode structure */
//        let s_block_group_nr =
            inner.read_u16::<LittleEndian>()?; /* block group # of this superblock */
//        let s_feature_compat =
            inner.read_u32::<LittleEndian>()?; /* compatible feature set */
        let s_feature_incompat =
            inner.read_u32::<LittleEndian>()?; /* incompatible feature set */

        let incompatible_features = IncompatibleFeature::from_bits(s_feature_incompat)
            .ok_or_else(|| parse_error(format!("completely unsupported feature flag: {:b}", s_feature_incompat)))?;

        let supported_incompatible_features =
            INCOMPAT_FILETYPE
                | INCOMPAT_EXTENTS
                | INCOMPAT_FLEX_BG
                | INCOMPAT_64BIT;

        if incompatible_features.intersects(!supported_incompatible_features) {
            return Err(parse_error(format!("some unsupported incompatible feature flags: {:?}",
                                           incompatible_features & !supported_incompatible_features)));
        }

        let long_structs = incompatible_features.contains(INCOMPAT_64BIT);

//        let s_feature_ro_compat =
            inner.read_u32::<LittleEndian>()?; /* readonly-compatible feature set */
        let mut s_uuid = [0; 16];
        inner.read_exact(&mut s_uuid)?; /* 128-bit uuid for volume */
        let mut s_volume_name = [0u8; 16];
        inner.read_exact(&mut s_volume_name)?; /* volume name */
        let mut s_last_mounted = [0u8; 64];
        inner.read_exact(&mut s_last_mounted)?; /* directory where last mounted */
//        let s_algorithm_usage_bitmap =
            inner.read_u32::<LittleEndian>()?; /* For compression */
//        let s_prealloc_blocks =
            inner.read_u8()?; /* Nr of blocks to try to preallocate*/
//        let s_prealloc_dir_blocks =
            inner.read_u8()?; /* Nr to preallocate for dirs */
//        let s_reserved_gdt_blocks =
            inner.read_u16::<LittleEndian>()?; /* Per group desc for online growth */
        let mut s_journal_uuid = [0u8; 16];
        inner.read_exact(&mut s_journal_uuid)?; /* uuid of journal superblock */
//        let s_journal_inum =
            inner.read_u32::<LittleEndian>()?; /* inode number of journal file */
//        let s_journal_dev =
            inner.read_u32::<LittleEndian>()?; /* device number of journal file */
//        let s_last_orphan =
            inner.read_u32::<LittleEndian>()?; /* start of list of inodes to delete */
        let mut s_hash_seed = [0u8; 4 * 4];
        inner.read_exact(&mut s_hash_seed)?; /* HTREE hash seed */
//        let s_def_hash_version =
            inner.read_u8()?; /* Default hash version to use */
//        let s_jnl_backup_type =
            inner.read_u8()?;
        let s_desc_size =
            inner.read_u16::<LittleEndian>()?; /* size of group descriptor */
//        let s_default_mount_opts =
            inner.read_u32::<LittleEndian>()?;
//        let s_first_meta_bg =
            inner.read_u32::<LittleEndian>()?; /* First metablock block group */
//        let s_mkfs_time =
            inner.read_u32::<LittleEndian>()?; /* When the filesystem was created */
        let mut s_jnl_blocks = [0; 17 * 4];
        inner.read_exact(&mut s_jnl_blocks)?; /* Backup of the journal inode */

        let s_blocks_count_hi =
            if !long_structs { None } else {
                Some(inner.read_u32::<LittleEndian>()?) /* Blocks count */
            };
////        let s_r_blocks_count_hi =
//            if !long_structs { None } else {
//                Some(inner.read_u32::<LittleEndian>()?) /* Reserved blocks count */
//            };
////        let s_free_blocks_count_hi =
//            if !long_structs { None } else {
//                Some(inner.read_u32::<LittleEndian>()?) /* Free blocks count */
//            };
////        let s_min_extra_isize =
//            if !long_structs { None } else {
//                Some(inner.read_u16::<LittleEndian>()?) /* All inodes have at least # bytes */
//            };
////        let s_want_extra_isize =
//            if !long_structs { None } else {
//                Some(inner.read_u16::<LittleEndian>()?) /* New inodes should reserve # bytes */
//            };
////        let s_flags =
//            if !long_structs { None } else {
//                Some(inner.read_u32::<LittleEndian>()?) /* Miscellaneous flags */
//            };

        if EXT4_SUPER_MAGIC != s_magic {
            return Err(parse_error(format!("invalid magic number: {:x} should be {:x}", s_magic, EXT4_SUPER_MAGIC)));
        }

        if 0 != s_creator_os {
            return Err(parse_error(format!("only support filesystems created on linux, not '{}'", s_creator_os)));
        }

        {
            const S_STATE_UNMOUNTED_CLEANLY: u16 = 0b01;
            const S_STATE_ERRORS_DETECTED: u16 = 0b10;

            if s_state & S_STATE_UNMOUNTED_CLEANLY == 0 || s_state & S_STATE_ERRORS_DETECTED != 0 {
                return Err(parse_error(format!("filesystem is not in a clean state: {:b}", s_state)));
            }
        }

        if 0 == s_inodes_per_group {
            return Err(parse_error("inodes per group cannot be zero".to_string()));
        }

        let block_size: u32 = match s_log_block_size {
            0 => 1024,
            1 => 2048,
            2 => 4096,
            6 => 65536,
            _ => {
                return Err(parse_error(format!("unexpected block size: 2^{}", s_log_block_size + 10)));
            }
        };

        if !long_structs {
            assert_eq!(0, s_desc_size);
        }

        if 1 != s_rev_level {
            return Err(parse_error(format!("unsupported rev_level {}", s_rev_level)));
        }

        let group_table_pos = if 1024 == block_size {
            // for 1k blocks, the table is in the third block, after:
            1024   // boot sector
            + 1024 // superblock
        } else {
            // for other blocks, the boot sector is in the first 1k of the first block,
            // followed by the superblock (also in first block), and the group table is afterwards
            block_size
        };

        inner.seek(io::SeekFrom::Start(group_table_pos as u64))?;
        let blocks_count = (
            s_blocks_count_lo as u64
            + ((s_blocks_count_hi.unwrap_or(0) as u64) << 32)
            - s_first_data_block as u64 + s_blocks_per_group as u64 - 1
        ) / s_blocks_per_group as u64;

        let mut groups = Vec::with_capacity(blocks_count as usize);

        for block in 0..blocks_count {
//            let bg_block_bitmap_lo =
                inner.read_u32::<LittleEndian>()?; /* Blocks bitmap block */
//            let bg_inode_bitmap_lo =
                inner.read_u32::<LittleEndian>()?; /* Inodes bitmap block */
            let bg_inode_table_lo =
                inner.read_u32::<LittleEndian>()?; /* Inodes table block */
//            let bg_free_blocks_count_lo =
                inner.read_u16::<LittleEndian>()?; /* Free blocks count */
            let bg_free_inodes_count_lo =
                inner.read_u16::<LittleEndian>()?; /* Free inodes count */
//            let bg_used_dirs_count_lo =
                inner.read_u16::<LittleEndian>()?; /* Directories count */
            let bg_flags =
                inner.read_u16::<LittleEndian>()?; /* EXT4_BG_flags (INODE_UNINIT, etc) */
//            let bg_exclude_bitmap_lo =
                inner.read_u32::<LittleEndian>()?; /* Exclude bitmap for snapshots */
//            let bg_block_bitmap_csum_lo =
                inner.read_u16::<LittleEndian>()?; /* crc32c(s_uuid+grp_num+bbitmap) LE */
//            let bg_inode_bitmap_csum_lo =
                inner.read_u16::<LittleEndian>()?; /* crc32c(s_uuid+grp_num+ibitmap) LE */
//            let bg_itable_unused_lo =
                inner.read_u16::<LittleEndian>()?; /* Unused inodes count */
//            let bg_checksum =
                inner.read_u16::<LittleEndian>()?; /* crc16(sb_uuid+group+desc) */

//            let bg_block_bitmap_hi =
                if s_desc_size < 4 { None } else {
                    Some(inner.read_u32::<LittleEndian>()?) /* Blocks bitmap block MSB */
                };
//            let bg_inode_bitmap_hi =
                if s_desc_size < 4 + 4 { None } else {
                    Some(inner.read_u32::<LittleEndian>()?) /* Inodes bitmap block MSB */
                };
            let bg_inode_table_hi =
                if s_desc_size < 4 + 4 + 4 { None } else {
                    Some(inner.read_u32::<LittleEndian>()?) /* Inodes table block MSB */
                };
//            let bg_free_blocks_count_hi =
                if s_desc_size < 4 + 4 + 4 + 2 { None } else {
                    Some(inner.read_u16::<LittleEndian>()?) /* Free blocks count MSB */
                };
            let bg_free_inodes_count_hi =
                if s_desc_size < 4 + 4 + 4 + 2 + 2 { None } else {
                    Some(inner.read_u16::<LittleEndian>()?) /* Free inodes count MSB */
                };

//          let bg_used_dirs_count_hi =
//              inner.read_u16::<LittleEndian>()?; /* Directories count MSB */
//          let bg_itable_unused_hi =
//              inner.read_u16::<LittleEndian>()?; /* Unused inodes count MSB */
//          let bg_exclude_bitmap_hi =
//              inner.read_u32::<LittleEndian>()?; /* Exclude bitmap block MSB */
//          let bg_block_bitmap_csum_hi =
//              inner.read_u16::<LittleEndian>()?; /* crc32c(s_uuid+grp_num+bbitmap) BE */
//          let bg_inode_bitmap_csum_hi =
//              inner.read_u16::<LittleEndian>()?; /* crc32c(s_uuid+grp_num+ibitmap) BE */

            if s_desc_size > 16 {
                inner.seek(io::SeekFrom::Current((s_desc_size - 16) as i64))?;
            }

            let inode_table_block = bg_inode_table_lo as u64
                | ((bg_inode_table_hi.unwrap_or(0) as u64) << 32);
            let free_inodes_count = bg_free_inodes_count_lo as u32
                | ((bg_free_inodes_count_hi.unwrap_or(0) as u32) << 16);

            let unallocated = bg_flags & EXT4_BLOCK_GROUP_INODES_UNUSED != 0 || bg_flags & EXT4_BLOCK_GROUP_BLOCKS_UNUSED != 0;

            if free_inodes_count > s_inodes_per_group {
                return Err(parse_error(format!("too many free inodes in group {}: {} > {}",
                                               block, free_inodes_count, s_inodes_per_group)));
            }

            let inodes = if unallocated {
                0
            } else {
                s_inodes_per_group - free_inodes_count
            };

            groups.push(BlockGroup {
                inode_table_block,
                inodes,
            });
        }

        Ok(SuperBlock {
            block_size,
            inode_size: s_inode_size,
            inodes_per_group: s_inodes_per_group,
            groups,
        })
    }

    fn load_inode<R>(&self, inner: &mut R, inode: u32) -> io::Result<Inode>
        where R: io::Read + io::Seek {
        assert_ne!(0, inode);

        {
            let inode = inode - 1;
            let group_number = inode / self.inodes_per_group;
            let group = &self.groups[group_number as usize];
            let inode_index_in_group = inode % self.inodes_per_group;
            assert!(inode_index_in_group < group.inodes,
                    "inode <{}> number must fit in group: {} is greater than {} for group {}",
                    inode + 1,
                    inode_index_in_group, group.inodes, group_number);
            let block = group.inode_table_block;
            let pos = block * self.block_size as u64 + inode_index_in_group as u64 * self.inode_size as u64;
            inner.seek(io::SeekFrom::Start(pos))?;
        }

        let i_mode =
            inner.read_u16::<LittleEndian>()?; /* File mode */
        let i_uid =
            inner.read_u16::<LittleEndian>()?; /* Low 16 bits of Owner Uid */
        let i_size_lo =
            inner.read_u32::<LittleEndian>()?; /* Size in bytes */
        let i_atime =
            inner.read_u32::<LittleEndian>()?; /* Access time */
        let i_ctime =
            inner.read_u32::<LittleEndian>()?; /* Inode Change time */
        let i_mtime =
            inner.read_u32::<LittleEndian>()?; /* Modification time */
//      let i_dtime =
            inner.read_u32::<LittleEndian>()?; /* Deletion Time */
        let i_gid =
            inner.read_u16::<LittleEndian>()?; /* Low 16 bits of Group Id */
        let i_links_count =
            inner.read_u16::<LittleEndian>()?; /* Links count */
//      let i_blocks_lo =
            inner.read_u32::<LittleEndian>()?; /* Blocks count */
        let i_flags =
            inner.read_u32::<LittleEndian>()?; /* File flags */
//      let l_i_version =
        inner.read_u32::<LittleEndian>()?;

        let mut block = [0u8; 15 * 4];
            inner.read_exact(&mut block)?; /* Pointers to blocks */

//      let i_generation =
            inner.read_u32::<LittleEndian>()?; /* File version (for NFS) */
//      let i_file_acl_lo =
            inner.read_u32::<LittleEndian>()?; /* File ACL */
        let i_size_high =
            inner.read_u32::<LittleEndian>()?;
//      let i_obso_faddr =
            inner.read_u32::<LittleEndian>()?; /* Obsoleted fragment address */
//      let l_i_blocks_high =
            inner.read_u16::<LittleEndian>()?;
//      let l_i_file_acl_high =
            inner.read_u16::<LittleEndian>()?;
        let l_i_uid_high =
            inner.read_u16::<LittleEndian>()?;
        let l_i_gid_high =
            inner.read_u16::<LittleEndian>()?;
//      let l_i_checksum_lo =
            inner.read_u16::<LittleEndian>()?; /* crc32c(uuid+inum+inode) LE */
//      let l_i_reserved =
            inner.read_u16::<LittleEndian>()?;
        let i_extra_isize =
            inner.read_u16::<LittleEndian>()?;

//      let i_checksum_hi =
            if i_extra_isize < 2 { None } else {
                Some(inner.read_u16::<BigEndian>()?) /* crc32c(uuid+inum+inode) BE */
            };
        let i_ctime_extra =
            if i_extra_isize < 2 + 4 { None } else {
                Some(inner.read_u32::<LittleEndian>()?) /* extra Change time      (nsec << 2 | epoch) */
            };
        let i_mtime_extra =
            if i_extra_isize < 2 + 4 + 4 { None } else {
                Some(inner.read_u32::<LittleEndian>()?) /* extra Modification time(nsec << 2 | epoch) */
            };
        let i_atime_extra =
            if i_extra_isize < 2 + 4 + 4 + 4 { None } else {
                Some(inner.read_u32::<LittleEndian>()?) /* extra Access time      (nsec << 2 | epoch) */
            };
        let i_crtime =
            if i_extra_isize < 2 + 4 + 4 + 4 + 4 { None } else {
                Some(inner.read_u32::<LittleEndian>()?) /* File Creation time */
            };
        let i_crtime_extra =
            if i_extra_isize < 2 + 4 + 4 + 4 + 4 + 4 { None } else {
                Some(inner.read_u32::<LittleEndian>()?) /* extra FileCreationtime (nsec << 2 | epoch) */
            };
//      let i_version_hi =
            if i_extra_isize < 2 + 4 + 4 + 4 + 4 + 4 + 4 { None } else {
               Some(inner.read_u32::<LittleEndian>()?) /* high 32 bits for 64-bit version */
            };
//      let i_projid =
            if i_extra_isize < 2 + 4 + 4 + 4 + 4 + 4 + 4 + 4 { None } else {
                Some(inner.read_u32::<LittleEndian>()?) /* Project ID */
            };

        // TODO: there could be extended attributes to read here

        let stat = Stat {
            extracted_type: FileType::from_mode(i_mode)
                .ok_or_else(|| parse_error(format!("unexpected file type in mode: {:b}", i_mode)))?,
            file_mode: i_mode & 0b111_111_111_111,
            uid: i_uid as u32 | ((l_i_uid_high as u32) << 16),
            gid: i_gid as u32 | ((l_i_gid_high as u32) << 16),
            size: (i_size_lo as u64) | ((i_size_high as u64) << 32),
            atime: Time {
                epoch_secs: i_atime,
                nanos: i_atime_extra,
            },
            ctime: Time {
                epoch_secs: i_ctime,
                nanos: i_ctime_extra,
            },
            mtime: Time {
                epoch_secs: i_mtime,
                nanos: i_mtime_extra,
            },
            btime: i_crtime.map(|epoch_secs| Time {
                epoch_secs,
                nanos: i_crtime_extra,
            }),
            link_count: i_links_count,
        };

        Ok(Inode {
            stat,
            number: inode,
            flags: InodeFlags::from_bits(i_flags)
                .expect("unrecognised inode flags"),
            block,
        })
    }

    fn add_found_extents<R>(
        &self,
        inner: &mut R,
        block: &[u8],
        expected_depth: u16,
        extents: &mut Vec<Extent>) -> io::Result<()>
    where R: io::Read + io::Seek {

        assert_eq!(0x0a, block[0]);
        assert_eq!(0xf3, block[1]);

        let extent_entries = as_u16(&block[2..]);
        // 4..: max; doesn't seem to be useful during read
        let depth = as_u16(&block[6..]);
        // 8..: generation, not used in standard ext4

        assert_eq!(expected_depth, depth);

        if 0 == depth {
            for en in 0..extent_entries {
                let raw_extent = &block[12 + en as usize * 12..];
                let ee_block = as_u32(raw_extent);
                let ee_len = as_u16(&raw_extent[4..]);
                let ee_start_hi = as_u16(&raw_extent[6..]);
                let ee_start_lo = as_u32(&raw_extent[8..]);
                let ee_start = ee_start_lo as u64 + 0x1000 * ee_start_hi as u64;

                extents.push(Extent {
                    block: ee_block,
                    start: ee_start,
                    len: ee_len,
                });
            }

            return Ok(());
        }

        for en in 0..extent_entries {
            let extent_idx = &block[12 + en as usize * 12..];
//            let ei_block = as_u32(extent_idx);
            let ei_leaf_lo = as_u32(&extent_idx[4..]);
            let ei_leaf_hi = as_u16(&extent_idx[8..]);
            let ee_leaf: u64 = ei_leaf_lo as u64 + ((ei_leaf_hi as u64) << 32);
            inner.seek(io::SeekFrom::Start(self.block_size as u64 * ee_leaf))?;
            let mut block = vec![0u8; self.block_size as usize];
            inner.read_exact(&mut block)?;
            self.add_found_extents(inner, &block, depth - 1, extents)?;
        }

        Ok(())
    }

    fn load_extent_tree<R>(&self, inner: &mut R, start: [u8; 4 * 15]) -> io::Result<Vec<Extent>>
    where R: io::Read + io::Seek {
        assert_eq!(0x0a, start[0]);
        assert_eq!(0xf3, start[1]);

        let extent_entries = as_u16(&start[2..]);
        // 4..: max; doesn't seem to be useful during read
        let depth = as_u16(&start[6..]);

        assert!(depth <= 5);

        let mut extents = Vec::with_capacity(extent_entries as usize + depth as usize * 200);

        self.add_found_extents(inner, &start, depth, &mut extents)?;

        extents.sort_by_key(|e| e.block);

        Ok(extents)
    }



    fn read_directory<R>(&self, inner: &mut R, inode: &Inode) -> io::Result<Vec<DirEntry>>
    where R: io::Read + io::Seek {

        let mut dirs = Vec::with_capacity(40);

        let data = {
            // if the flags, minus irrelevant flags, isn't just EXTENTS...
            if !inode.only_relevant_flag_is_extents() {
                return Err(parse_error(format!("inode without unsupported flags: {0:x} {0:b}", inode.flags)));
            }

            self.load_all(inner, inode)?
        };

        let total_len = data.len();

        let mut cursor = io::Cursor::new(data);
        let mut read = 0usize;
        loop {
            let child_inode = cursor.read_u32::<LittleEndian>()?;
            let rec_len = cursor.read_u16::<LittleEndian>()?;
            let name_len = cursor.read_u8()?;
            let file_type = cursor.read_u8()?;
            let mut name = vec![0u8; name_len as usize];
            cursor.read_exact(&mut name)?;
            cursor.seek(io::SeekFrom::Current(rec_len as i64 - name_len as i64 - 4 - 2 - 2))?;
            if 0 != child_inode {
                let name = std::str::from_utf8(&name).map_err(|e|
                    parse_error(format!("invalid utf-8 in file name: {}", e)))?;

                dirs.push(DirEntry {
                    inode: child_inode,
                    name: name.to_string(),
                    file_type: FileType::from_dir_hint(file_type)
                        .expect("valid file type"),
                });
            }

            read += rec_len as usize;
            if read >= total_len {
                assert_eq!(read, total_len);
                break;
            }
        }

        Ok(dirs)
    }

    pub fn root<R>(&self, mut inner: &mut R) -> io::Result<Inode>
        where R: io::Read + io::Seek {
        self.load_inode(inner, 2)
    }

    pub fn walk<R>(&self, mut inner: &mut R, inode: &Inode, path: String) -> io::Result<()>
        where R: io::Read + io::Seek {
        let enhanced = self.enhance(inner, inode)?;

        println!("{}: {:?} {:?}", path, enhanced, inode.stat);

        if let Enhanced::Directory(entries) = enhanced {
            for entry in entries {
                if "." == entry.name || ".." == entry.name {
                    continue;
                }

                let child_node = self.load_inode(inner, entry.inode)?;
                self.walk(inner, &child_node, format!("{}/{}", path, entry.name))?;
            }
        }

//    self.walk(inner, &i, format!("{}/{}", path, entry.name)).map_err(|e|
//    parse_error(format!("while processing {}: {}", path, e)))?;

        Ok(())
    }

    pub fn enhance<R>(&self, mut inner: &mut R, inode: &Inode) -> io::Result<Enhanced>
        where R: io::Read + io::Seek {
        Ok(match inode.stat.extracted_type {
            FileType::RegularFile => Enhanced::RegularFile,
            FileType::Socket => Enhanced::Socket,
            FileType::Fifo => Enhanced::Fifo,

            FileType::Directory => Enhanced::Directory(self.read_directory(inner, inode)?),
            FileType::SymbolicLink =>
                Enhanced::SymbolicLink(if inode.stat.size < 60 {
                    assert!(inode.flags.is_empty());
                    std::str::from_utf8(&inode.block[0..inode.stat.size as usize]).expect("utf-8").to_string()
                } else {
                    assert!(inode.only_relevant_flag_is_extents());
                    std::str::from_utf8(&self.load_all(inner, inode)?).expect("utf-8").to_string()
                }),
            FileType::CharacterDevice => {
                let (maj, min) = load_maj_min(inode.block);
                Enhanced::CharacterDevice(maj, min)
            }
            FileType::BlockDevice => {
                let (maj, min) = load_maj_min(inode.block);
                Enhanced::BlockDevice(maj, min)
            }
        })
    }

    fn load_all<R>(&self, inner: &mut R, inode: &Inode) -> io::Result<Vec<u8>>
    where R: io::Read + io::Seek {

        #[allow(unknown_lints, absurd_extreme_comparisons)] {
            // this check only makes sense on non-64-bit platforms; on 64-bit usize == u64.
            if inode.stat.size > std::usize::MAX as u64 {
                return Err(io::Error::new(io::ErrorKind::InvalidData,
                                          format!("file is too big for this platform to fit in memory: {}",
                                                  inode.stat.size)));
            }
        }

        let size = inode.stat.size as usize;

        let mut ret = Vec::with_capacity(size);

        assert_eq!(size, self.reader_for(inner, inode)?.read_to_end(&mut ret)?);

        Ok(ret)
    }


    fn reader_for<R>(&self, mut inner: R, inode: &Inode) -> io::Result<TreeReader<R>>
    where R: io::Read + io::Seek {
        let extents = self.load_extent_tree(&mut inner, inode.block)?;

        inner.seek(io::SeekFrom::Start(extents[0].start as u64 * self.block_size as u64))?;

        assert_eq!(0, extents[0].block);

        Ok(TreeReader {
            pos: 0,
            inner,
            extents,
            block_size: self.block_size,
            sparse_bytes: None,
        })
    }
}

impl<R> io::Read for TreeReader<R>
where R: io::Read + io::Seek {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if 0 == buf.len() || self.extents.is_empty() {
            return Ok(0);
        }

        // we're feeding them some sparse bytes, keep doing so, and mark as done if we're done
        if let Some(remaining_sparse) = self.sparse_bytes {
            return if (buf.len() as u64) < remaining_sparse {
                self.sparse_bytes = Some(remaining_sparse - buf.len() as u64);
                zero(buf);
                Ok(buf.len())
            } else {
                self.sparse_bytes = None;
                zero(&mut buf[0..remaining_sparse as usize]);
                Ok(remaining_sparse as usize)
            };
        }

        // we must be feeding them a real extent; keep doing so
        let read;
        {
            // first self.extents is the block we're reading from
            // we've read self.pos from it already
            let reading_extent = &self.extents[0];
            let this_extent_len_bytes = reading_extent.len as u64 * self.block_size as u64;

            let bytes_until_end = this_extent_len_bytes - self.pos;

            let to_read = std::cmp::min(buf.len() as u64, bytes_until_end) as usize;

            read = self.inner.read(&mut buf[0..to_read])?;
            assert_ne!(0, read);

            // if, while reading, we didn't reach the end of this extent, everything is okay
            if (read as u64) != bytes_until_end {
                self.pos += read as u64;
                return Ok(read);
            }
        }

        // we finished reading the current extent
        let last = self.extents.remove(0);

        if !self.extents.is_empty() {
            let next = &self.extents[0];

            // check for HOLES
            let last_ended = last.block as u64 + last.len as u64;
            let new_starts = next.block as u64;
            let hole_size = (new_starts - last_ended) * self.block_size as u64;
            if 0 != hole_size {
                // before feeding them the next extent, lets feed them the hole
                self.sparse_bytes = Some(hole_size);
            }
        }

        Ok(read)
    }
}

fn zero(buf: &mut [u8]) {
    for i in 0..buf.len() {
        buf[i] = 0;
    }
}

fn load_maj_min(block: [u8; 4 * 15]) -> (u16, u32) {
    if 0 != block[0] || 0 != block[1] {
        (block[1] as u16, block[0] as u32)
    } else {
        // if you think reading this is bad, I had to write it
        (block[5] as u16
            | (((block[6] & 0b0000_1111) as u16) << 8),
        block[4] as u32
            | ((block[7] as u32) << 12)
            | (((block[6] & 0b1111_0000) as u32) >> 4) << 8)
    }
}

impl Inode {
    fn only_relevant_flag_is_extents(&self) -> bool {
        self.flags & (
            INODE_COMPR
            | INODE_DIRTY
            | INODE_COMPRBLK
            | INODE_ENCRYPT
            | INODE_IMAGIC
            | INODE_NOTAIL
            | INODE_TOPDIR
            | INODE_HUGE_FILE
            | INODE_EXTENTS
            | INODE_EA_INODE
            | INODE_EOFBLOCKS
            | INODE_INLINE_DATA
        ) == INODE_EXTENTS
    }
}

fn as_u16(buf: &[u8]) -> u16 {
    buf[0] as u16 + buf[1] as u16 * 0x100
}

fn as_u32(buf: &[u8]) -> u32 {
    as_u16(buf) as u32 + as_u16(&buf[2..]) as u32 * 0x10000
}

fn parse_error(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg)
}
