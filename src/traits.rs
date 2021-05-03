use crate::error::{NtfsError, Result};
use binread::io;
use binread::io::{Read, Seek, SeekFrom};

pub trait NtfsReadSeek {
    fn read<T>(&mut self, fs: &mut T, buf: &mut [u8]) -> Result<usize>
    where
        T: Read + Seek;

    fn read_exact<T>(&mut self, fs: &mut T, mut buf: &mut [u8]) -> Result<()>
    where
        T: Read + Seek,
    {
        // This implementation is taken from https://github.com/rust-lang/rust/blob/5662d9343f0696efcc38a1264656737c9f22d427/library/std/src/io/mod.rs
        // It handles all corner cases properly and outputs the known `io` error messages.
        while !buf.is_empty() {
            match self.read(fs, buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf = &mut buf[n..];
                }
                Err(NtfsError::Io(e)) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }

        if !buf.is_empty() {
            Err(NtfsError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            )))
        } else {
            Ok(())
        }
    }

    fn seek<T>(&mut self, fs: &mut T, pos: SeekFrom) -> Result<u64>
    where
        T: Read + Seek;

    /// See [`std::io::Seek::stream_position`].
    fn stream_position(&mut self) -> Result<u64>;
}
