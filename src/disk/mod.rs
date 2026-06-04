use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub struct DiskImage {
    file: File,
    pub sector_size: u64,
}

impl DiskImage {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        Ok(Self {
            file,
            sector_size: 512,
        })
    }

    pub fn read_sector(&mut self, sector_number: u64) -> io::Result<Vec<u8>> {
        let offset = sector_number * self.sector_size;

        self.file.seek(SeekFrom::Start(offset))?;

        let mut buffer = vec![0u8; self.sector_size as usize];

        self.file.read_exact(&mut buffer)?;

        Ok(buffer)
    }
}
