use crate::disk::DiskImage;
use std::path::Path;

mod disk;

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

    match image.read_sector(0) {
        Ok(sector) => {
            println!("First sector 0: {:?}", sector);
        }
        Err(e) => {
            eprintln!("Failed to read sector: {}", e);
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
