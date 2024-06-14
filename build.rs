use std::{cmp::max, collections::HashMap, fs::File, io, path::{Path, PathBuf}, process::Command};

use fatfs::{FileSystem, FormatVolumeOptions, FsOptions};

const UEFI_BOOT_FILENAME: &str = "/efi/boot/bootx64.efi";
const KERNEL_BOOT_FILENAME: &str = "/kernel.elf";
const MB: u64 = 1024 * 1024;

// TODO: Support BIOS booting

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let kernel = PathBuf::from(std::env::var("CARGO_BIN_FILE_KERNEL").unwrap());
    let efi = PathBuf::from(std::env::var("CARGO_BIN_FILE_BOOTLOADER").unwrap());
    let img = out_dir.join("uefi.img");
    create_uefi_image(&kernel, &efi, &img);
    println!("cargo:rustc-env=UEFI_PATH={}", img.display());
}

fn create_uefi_image(kernel: &Path, efi: &Path, img: &Path) {
    let mut files = HashMap::new();
    files.insert(UEFI_BOOT_FILENAME, efi);
    files.insert(KERNEL_BOOT_FILENAME, kernel);
    let fat_file = tempfile::NamedTempFile::new().unwrap();
    create_fat_filesystem(files, &fat_file.as_file());
    // For some reason parted throws an error whenever the GPT disk is less than 4M
    let img_size = max(4 * MB, (fat_file.as_file().metadata().unwrap().len() + MB / 4).div_ceil(MB) * MB);
    Command::new("./mkgpt.sh")
        .arg(img.as_os_str())
        .arg(fat_file.path())
        .arg(img_size.to_string())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
    fat_file.close().unwrap();
}

fn create_fat_filesystem(files: HashMap<&str, &Path>, file: &File) {
    let mut nsize = 0;
    for source in files.values() {
        nsize += source.metadata().unwrap().len();
    }

    file.set_len(nsize.div_ceil(MB) * MB).unwrap();
    fatfs::format_volume(file, FormatVolumeOptions::new()).unwrap();
    let filesystem = FileSystem::new(file, FsOptions::new()).unwrap();
    let root_dir = filesystem.root_dir();

    for (path, source) in files {
        let target_path = Path::new(&path);
        for ancestor in target_path.ancestors().collect::<Vec<_>>().iter().skip(1).rev().skip(1) {
            root_dir.create_dir(&ancestor.to_str().unwrap()).unwrap();
        }

        let mut new_file = root_dir.create_file(path).unwrap();
        new_file.truncate().unwrap();
        io::copy(&mut File::open(source).unwrap(), &mut new_file).unwrap();
    }
}
