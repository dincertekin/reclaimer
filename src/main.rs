use std::path::Path;

mod disk;
mod scanner;

use disk::DiskImage;

fn main() {
    println!("Reclaimer - File Recovery Tool");
    println!("-------------------------------");

    let path = Path::new("test.img");
    let output_dir = Path::new("recovered");

    let mut image = match DiskImage::open(path) {
        Ok(image) => image,
        Err(e) => {
            eprintln!("Failed to open disk image: {}", e);
            return;
        }
    };

    println!("Scanning for file signatures...\n");

    let mut sector_number = 0u64;
    let mut found_count = 0u32;

    loop {
        match image.read_sector(sector_number) {
            Ok(sector) => {
                if let Some(sig) = scanner::detect_signature(&sector) {
                    found_count += 1;
                    println!(
                        "[{}] Found {} at sector {} (byte offset {})",
                        found_count,
                        sig.name,
                        sector_number,
                        sector_number * 512
                    );

                    match scanner::extract_file(
                        &sector,
                        sig,
                        &mut image,
                        sector_number,
                        output_dir,
                        found_count,
                    ) {
                        Ok(filename) => println!("Saved as: {}", filename),
                        Err(e) => eprintln!("Failed to save: {}", e),
                    }
                }
                sector_number += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                println!("\nScan complete. Reached end of image.");
                println!("Total found: {}", found_count);
                break;
            }
            Err(e) => {
                eprintln!("Read error at sector {}: {}", sector_number, e);
                break;
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
