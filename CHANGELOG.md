# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [0.2.0] - 2022-04-14
- Fix accessing attributes of zero length (#6)
- Check BPB-reported sizes more thoroughly and improve related error messages (#1, #4)
- Add support for Index Roots that are part of an Attribute List (#7)
- Fix reading empty volume name strings (#9)
- Fix accessing Subnode VCN 0 in an Index Allocation that is part of an Attribute List (#10, #11)
- Tighten the cluster size and record size limits for safety (#2)
- Introduce `NtfsPosition` for all `position` and `data_position` values
- Properly handle `sectors_per_cluster` for cluster sizes > 64K, up to 2M (#12, #13)
- Support native sector sizes up to 4K in the BPB and `ntfs-shell`, and use `NTFS_BLOCK_SIZE` instead of the partition's sector size for Record Fixup (#14)
- Replace `chrono` by the better maintained and no_std compatible `time`
- Update to Rust 2021, MSRV to 1.60, use new features where appropriate
- Upgrade dependencies


## [0.1.0] - 2022-01-14
- Initial release
