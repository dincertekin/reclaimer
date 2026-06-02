use std::path::Path;

mod disk;
use crate::disk::DiskImage;

const JPEG_BYTES: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0]; // This is where magic begins :D

fn main() {
    println!("Reclaimer - File Recovery Tool");
    println!("-------------------------------");

    let path = Path::new("test.img");

    let mut image = match DiskImage::open(path) {
        Ok(image) => image,
        Err(e) => {
            eprintln!("Failed to open disk image: {}", e);
            return;
        }
    };

    println!("Scanning for JPEG signatures...\n");

    let mut sector_number = 0;

    loop {
        match image.read_sector(sector_number) {
            Ok(sector) => {
                if sector.starts_with(&JPEG_BYTES) {
                    println!(
                        "Found JPEG at sector {} (offset: {})",
                        sector_number,
                        sector_number * 512
                    );
                }

                sector_number += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                println!("\nScan complete. Reached end of image.");
                break;
            }
            Err(e) => {
                eprintln!("Failed to read sector: {}", e);
            }
        }
    }
}

#[allow(dead_code)]
fn print_hex(data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        print!("{:04X} ", i * 16);
        for byte in chunk {
            print!("{:02X} ", byte);
        }
        println!();
    }
}
