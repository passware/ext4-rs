use std::io;

use ::Time;

use ::parse_error;

use byteorder::{ReadBytesExt, LittleEndian, BigEndian};

const EXT4_SUPER_MAGIC: u16 = 0xEF53;

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

pub fn superblock<R>(mut inner: R) -> io::Result<::SuperBlock>
where R: io::Read + io::Seek {

    // <a cut -c 9- | fgrep ' s_' | fgrep -v ERR_ | while read ty nam comment; do printf "let %s =\n  inner.read_%s::<LittleEndian>()?; %s\n" $(echo $nam | tr -d ';') $(echo $ty | sed 's/__le/u/; s/__//') $comment; done
//    let s_inodes_count =
        inner.read_u32::<LittleEndian>()?; /* Inodes count */
    let s_blocks_count_lo =
        inner.read_u32::<LittleEndian>()?; /* Blocks count */
//    let s_r_blocks_count_lo =
        inner.read_u32::<LittleEndian>()?; /* Reserved blocks count */
//    let s_free_blocks_count_lo =
        inner.read_u32::<LittleEndian>()?; /* Free blocks count */
//    let s_free_inodes_count =
        inner.read_u32::<LittleEndian>()?; /* Free inodes count */
    let s_first_data_block =
        inner.read_u32::<LittleEndian>()?; /* First Data Block */
    let s_log_block_size =
        inner.read_u32::<LittleEndian>()?; /* Block size */
//    let s_log_cluster_size =
        inner.read_u32::<LittleEndian>()?; /* Allocation cluster size */
    let s_blocks_per_group =
        inner.read_u32::<LittleEndian>()?; /* # Blocks per group */
//    let s_clusters_per_group =
        inner.read_u32::<LittleEndian>()?; /* # Clusters per group */
    let s_inodes_per_group =
        inner.read_u32::<LittleEndian>()?; /* # Inodes per group */
//    let s_mtime =
        inner.read_u32::<LittleEndian>()?; /* Mount time */
//    let s_wtime =
        inner.read_u32::<LittleEndian>()?; /* Write time */
//    let s_mnt_count =
        inner.read_u16::<LittleEndian>()?; /* Mount count */
//    let s_max_mnt_count =
        inner.read_u16::<LittleEndian>()?; /* Maximal mount count */
    let s_magic =
        inner.read_u16::<LittleEndian>()?; /* Magic signature */
    let s_state =
        inner.read_u16::<LittleEndian>()?; /* File system state */
//    let s_errors =
        inner.read_u16::<LittleEndian>()?; /* Behaviour when detecting errors */
//    let s_minor_rev_level =
        inner.read_u16::<LittleEndian>()?; /* minor revision level */
//    let s_lastcheck =
        inner.read_u32::<LittleEndian>()?; /* time of last check */
//    let s_checkinterval =
        inner.read_u32::<LittleEndian>()?; /* max. time between checks */
    let s_creator_os =
        inner.read_u32::<LittleEndian>()?; /* OS */
    let s_rev_level =
        inner.read_u32::<LittleEndian>()?; /* Revision level */
//    let s_def_resuid =
        inner.read_u16::<LittleEndian>()?; /* Default uid for reserved blocks */
//    let s_def_resgid =
        inner.read_u16::<LittleEndian>()?; /* Default gid for reserved blocks */
//    let s_first_ino =
        inner.read_u32::<LittleEndian>()?; /* First non-reserved inode */
    let s_inode_size =
        inner.read_u16::<LittleEndian>()?; /* size of inode structure */
//    let s_block_group_nr =
        inner.read_u16::<LittleEndian>()?; /* block group # of this superblock */
//    let s_feature_compat =
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

//    let s_feature_ro_compat =
        inner.read_u32::<LittleEndian>()?; /* readonly-compatible feature set */
    let mut s_uuid = [0; 16];
    inner.read_exact(&mut s_uuid)?; /* 128-bit uuid for volume */
    let mut s_volume_name = [0u8; 16];
    inner.read_exact(&mut s_volume_name)?; /* volume name */
    let mut s_last_mounted = [0u8; 64];
    inner.read_exact(&mut s_last_mounted)?; /* directory where last mounted */
//    let s_algorithm_usage_bitmap =
        inner.read_u32::<LittleEndian>()?; /* For compression */
//    let s_prealloc_blocks =
        inner.read_u8()?; /* Nr of blocks to try to preallocate*/
//    let s_prealloc_dir_blocks =
        inner.read_u8()?; /* Nr to preallocate for dirs */
//    let s_reserved_gdt_blocks =
        inner.read_u16::<LittleEndian>()?; /* Per group desc for online growth */
    let mut s_journal_uuid = [0u8; 16];
    inner.read_exact(&mut s_journal_uuid)?; /* uuid of journal superblock */
//    let s_journal_inum =
        inner.read_u32::<LittleEndian>()?; /* inode number of journal file */
//    let s_journal_dev =
        inner.read_u32::<LittleEndian>()?; /* device number of journal file */
//    let s_last_orphan =
        inner.read_u32::<LittleEndian>()?; /* start of list of inodes to delete */
    let mut s_hash_seed = [0u8; 4 * 4];
    inner.read_exact(&mut s_hash_seed)?; /* HTREE hash seed */
//    let s_def_hash_version =
        inner.read_u8()?; /* Default hash version to use */
//    let s_jnl_backup_type =
        inner.read_u8()?;
    let s_desc_size =
        inner.read_u16::<LittleEndian>()?; /* size of group descriptor */
//    let s_default_mount_opts =
        inner.read_u32::<LittleEndian>()?;
//    let s_first_meta_bg =
        inner.read_u32::<LittleEndian>()?; /* First metablock block group */
//    let s_mkfs_time =
        inner.read_u32::<LittleEndian>()?; /* When the filesystem was created */
    let mut s_jnl_blocks = [0; 17 * 4];
    inner.read_exact(&mut s_jnl_blocks)?; /* Backup of the journal inode */

    let s_blocks_count_hi =
        if !long_structs { None } else {
            Some(inner.read_u32::<LittleEndian>()?) /* Blocks count */
        };
////    let s_r_blocks_count_hi =
//        if !long_structs { None } else {
//            Some(inner.read_u32::<LittleEndian>()?) /* Reserved blocks count */
//        };
////    let s_free_blocks_count_hi =
//        if !long_structs { None } else {
//            Some(inner.read_u32::<LittleEndian>()?) /* Free blocks count */
//        };
////    let s_min_extra_isize =
//        if !long_structs { None } else {
//            Some(inner.read_u16::<LittleEndian>()?) /* All inodes have at least # bytes */
//        };
////    let s_want_extra_isize =
//        if !long_structs { None } else {
//            Some(inner.read_u16::<LittleEndian>()?) /* New inodes should reserve # bytes */
//        };
////    let s_flags =
//        if !long_structs { None } else {
//            Some(inner.read_u32::<LittleEndian>()?) /* Miscellaneous flags */
//        };

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

    let groups = ::block_groups::BlockGroups::new(inner, blocks_count,
                                                s_desc_size, s_inodes_per_group,
                                                block_size, s_inode_size)?;

    Ok(::SuperBlock {
        groups,
    })
}

pub fn inode<R>(mut inner: R, inode: u32, block_size: u32) -> io::Result<::Inode>
where R: io::Read + io::Seek {
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
//  let i_dtime =
        inner.read_u32::<LittleEndian>()?; /* Deletion Time */
    let i_gid =
        inner.read_u16::<LittleEndian>()?; /* Low 16 bits of Group Id */
    let i_links_count =
        inner.read_u16::<LittleEndian>()?; /* Links count */
//  let i_blocks_lo =
        inner.read_u32::<LittleEndian>()?; /* Blocks count */
    let i_flags =
        inner.read_u32::<LittleEndian>()?; /* File flags */
//  let l_i_version =
    inner.read_u32::<LittleEndian>()?;

    let mut block = [0u8; 15 * 4];
        inner.read_exact(&mut block)?; /* Pointers to blocks */

//  let i_generation =
        inner.read_u32::<LittleEndian>()?; /* File version (for NFS) */
//  let i_file_acl_lo =
        inner.read_u32::<LittleEndian>()?; /* File ACL */
    let i_size_high =
        inner.read_u32::<LittleEndian>()?;
//  let i_obso_faddr =
        inner.read_u32::<LittleEndian>()?; /* Obsoleted fragment address */
//  let l_i_blocks_high =
        inner.read_u16::<LittleEndian>()?;
//  let l_i_file_acl_high =
        inner.read_u16::<LittleEndian>()?;
    let l_i_uid_high =
        inner.read_u16::<LittleEndian>()?;
    let l_i_gid_high =
        inner.read_u16::<LittleEndian>()?;
//  let l_i_checksum_lo =
        inner.read_u16::<LittleEndian>()?; /* crc32c(uuid+inum+inode) LE */
//  let l_i_reserved =
        inner.read_u16::<LittleEndian>()?;
    let i_extra_isize =
        inner.read_u16::<LittleEndian>()?;

//  let i_checksum_hi =
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
//  let i_version_hi =
        if i_extra_isize < 2 + 4 + 4 + 4 + 4 + 4 + 4 { None } else {
           Some(inner.read_u32::<LittleEndian>()?) /* high 32 bits for 64-bit version */
        };
//  let i_projid =
        if i_extra_isize < 2 + 4 + 4 + 4 + 4 + 4 + 4 + 4 { None } else {
            Some(inner.read_u32::<LittleEndian>()?) /* Project ID */
        };

    // TODO: there could be extended attributes to read here

    let stat = ::Stat {
        extracted_type: ::FileType::from_mode(i_mode)
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

    Ok(::Inode {
        stat,
        number: inode,
        flags: ::InodeFlags::from_bits(i_flags)
            .expect("unrecognised inode flags"),
        block,
        block_size,
    })
}