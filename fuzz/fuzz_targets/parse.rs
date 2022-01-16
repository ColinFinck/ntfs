#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut data = std::io::Cursor::new(data);
    let mut fs = if let Ok(s) = ntfs::Ntfs::new(&mut data) {
        s
    } else {
        return;
    };

    if fs.read_upcase_table(&mut data).is_err() {
        return;
    }
});
