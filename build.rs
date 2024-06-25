use core::fmt::Debug;
use std::{borrow::Cow, collections::BTreeMap, fmt::Formatter, fs::{self, File}, io::{self, Cursor, Seek}, path::{Path, PathBuf}};
use bootloader_boot_config::{BootConfig, LevelFilter};
use fatfs::Dir;
use tempfile::NamedTempFile;

const UEFI_BOOT_FILENAME: &str = "/efi/boot/bootx64.efi";
const KERNEL_FILE_NAME: &str = "/kernel-x86_64";
const CONFIG_FILE_NAME: &str = "boot.json";

fn main() {
    let mut boot_config = BootConfig::default();
    boot_config.frame_buffer.minimum_framebuffer_width = Some(1024);
    boot_config.frame_buffer.minimum_framebuffer_height = Some(768);
    boot_config.log_level = LevelFilter::Trace;
    boot_config.frame_buffer_logging = true;
    boot_config.serial_logging = true;

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let kernel = PathBuf::from(std::env::var("CARGO_BIN_FILE_KERNEL").unwrap());
    let efi = PathBuf::from(std::env::var("CARGO_BIN_FILE_BOOTLOADER").unwrap());
    let img = out_dir.join("uefi.img");
    let image_builder = DiskImageBuilder::new(kernel.clone(), efi);
    image_builder.create_uefi_image(&img);
    println!("cargo:rustc-env=UEFI_PATH={}", img.display());
    println!("cargo:rustc-env=KERNEL_ELF={}", kernel.display());
}

pub struct DiskImageBuilder {
    files: BTreeMap<Cow<'static, str>, FileDataSource>,
}

impl DiskImageBuilder {
    /// Create a new instance of DiskImageBuilder, with the specified kernel.
    pub fn new(kernel: PathBuf, efi: PathBuf) -> Self {
        let mut obj = Self::empty();
        obj.set_kernel(kernel);
        obj.set_efi(efi);
        obj
    }

    /// Create a new, empty instance of DiskImageBuilder
    pub fn empty() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    /// Add or replace a kernel to be included in the final image.
    pub fn set_kernel(&mut self, path: PathBuf) -> &mut Self {
        self.set_file_source(KERNEL_FILE_NAME.into(), FileDataSource::File(path))
    }

    pub fn set_efi(&mut self, path: PathBuf) -> &mut Self {
        self.set_file_source(UEFI_BOOT_FILENAME.into(), FileDataSource::File(path))
    }

    /// Configures the runtime behavior of the bootloader.
    pub fn set_boot_config(&mut self, boot_config: &BootConfig) -> &mut Self {
        let json = serde_json::to_vec_pretty(boot_config).expect("failed to serialize BootConfig");
        self.set_file_source(CONFIG_FILE_NAME.into(), FileDataSource::Data(json))
    }

    /// Add a file with the specified bytes to the disk image
    ///
    /// Note that the bootloader only loads the kernel and ramdisk files into memory on boot.
    /// Other files need to be loaded manually by the kernel.
    pub fn set_file_contents(&mut self, destination: String, data: Vec<u8>) -> &mut Self {
        self.set_file_source(destination.into(), FileDataSource::Data(data))
    }

    /// Add a file with the specified source file to the disk image
    ///
    /// Note that the bootloader only loads the kernel and ramdisk files into memory on boot.
    /// Other files need to be loaded manually by the kernel.
    pub fn set_file(&mut self, destination: String, file_path: PathBuf) -> &mut Self {
        self.set_file_source(destination.into(), FileDataSource::File(file_path))
    }

    /// Create a GPT disk image for booting on UEFI systems.
    pub fn create_uefi_image(&self, image_path: &Path) {
        let fat_partition = self.create_fat_filesystem_image();
        create_gpt_disk(fat_partition.path(), image_path);
        fat_partition
            .close().unwrap();
    }

    /// Add a file source to the disk image
    fn set_file_source(
        &mut self,
        destination: Cow<'static, str>,
        source: FileDataSource,
    ) -> &mut Self {
        self.files.insert(destination, source);
        self
    }

    fn create_fat_filesystem_image(&self) -> NamedTempFile {
        let mut local_map: BTreeMap<&str, _> = BTreeMap::new();

        for (name, source) in &self.files {
            local_map.insert(name, source);
        }

        let out_file = NamedTempFile::new().unwrap();
        create_fat_filesystem(local_map, out_file.path());

        out_file
    }
}

#[derive(Clone)]
/// Defines a data source, either a source `std::path::PathBuf`, or a vector of bytes.
pub enum FileDataSource {
    File(PathBuf),
    Data(Vec<u8>),
    Bytes(&'static [u8]),
}

impl Debug for FileDataSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            FileDataSource::File(file) => {
                f.write_fmt(format_args!("data source: File {}", file.display()))
            }
            FileDataSource::Data(d) => {
                f.write_fmt(format_args!("data source: {} raw bytes ", d.len()))
            }
            FileDataSource::Bytes(b) => {
                f.write_fmt(format_args!("data source: {} raw bytes ", b.len()))
            }
        }
    }
}

impl FileDataSource {
    /// Get the length of the inner data source
    pub fn len(&self) -> u64 {
        match self {
            FileDataSource::File(path) => fs::metadata(path).unwrap()
                .len(),
            FileDataSource::Data(v) => v.len() as u64,
            FileDataSource::Bytes(s) => s.len() as u64,
        }
    }
    /// Copy this data source to the specified target that implements io::Write
    pub fn copy_to(&self, target: &mut dyn io::Write) {
        match self {
            FileDataSource::File(file_path) => {
                io::copy(
                    &mut fs::File::open(file_path).unwrap(),
                    target,
                ).unwrap();
            }
            FileDataSource::Data(contents) => {
                let mut cursor = Cursor::new(contents);
                io::copy(&mut cursor, target).unwrap();
            }
            FileDataSource::Bytes(contents) => {
                let mut cursor = Cursor::new(contents);
                io::copy(&mut cursor, target).unwrap();
            }
        };
    }
}

pub fn create_fat_filesystem(
    files: BTreeMap<&str, &FileDataSource>,
    out_fat_path: &Path,
) {
    const MB: u64 = 1024 * 1024;

    // calculate needed size
    let mut needed_size = 0;
    for source in files.values() {
        needed_size += source.len();
    }

    // create new filesystem image file at the given path and set its length
    let fat_file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(out_fat_path)
        .unwrap();
    let fat_size_padded_and_rounded = ((needed_size + 1024 * 64 - 1) / MB + 1) * MB + MB;
    fat_file.set_len(fat_size_padded_and_rounded).unwrap();

    // choose a file system label
    let mut label = *b"MY_RUST_OS!";

    // This __should__ always be a file, but maybe not. Should we allow the caller to set the volume label instead?
    if let Some(FileDataSource::File(path)) = files.get(KERNEL_FILE_NAME) {
        if let Some(name) = path.file_stem() {
            let converted = name.to_string_lossy();
            let name = converted.as_bytes();
            let mut new_label = [0u8; 11];
            let name = &name[..usize::min(new_label.len(), name.len())];
            let slice = &mut new_label[..name.len()];
            slice.copy_from_slice(name);
            label = new_label;
        }
    }

    // format the file system and open it
    let format_options = fatfs::FormatVolumeOptions::new().volume_label(label);
    fatfs::format_volume(&fat_file, format_options).unwrap();
    let filesystem = fatfs::FileSystem::new(&fat_file, fatfs::FsOptions::new()).unwrap();
    let root_dir = filesystem.root_dir();

    // copy files to file system
    add_files_to_image(&root_dir, files)
}

pub fn add_files_to_image(
    root_dir: &Dir<&File>,
    files: BTreeMap<&str, &FileDataSource>,
) {
    for (target_path_raw, source) in files {
        let target_path = Path::new(target_path_raw);
        // create parent directories
        let ancestors: Vec<_> = target_path.ancestors().skip(1).collect();
        for ancestor in ancestors.into_iter().rev().skip(1) {
            root_dir
                .create_dir(&ancestor.display().to_string()).unwrap();
        }

        let mut new_file = root_dir
            .create_file(target_path_raw).unwrap();
        new_file.truncate().unwrap();

        source.copy_to(&mut new_file);
    }

}

pub fn create_gpt_disk(fat_image: &Path, out_gpt_path: &Path) {
    // create new file
    let mut disk = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(out_gpt_path).unwrap();

    // set file size
    let partition_size: u64 = fs::metadata(fat_image).unwrap()
        .len();
    let disk_size = partition_size + 1024 * 64; // for GPT headers
    disk.set_len(disk_size).unwrap();

    // create a protective MBR at LBA0 so that disk is not considered
    // unformatted on BIOS systems
    let mbr = gpt::mbr::ProtectiveMBR::with_lb_size(
        u32::try_from((disk_size / 512) - 1).unwrap_or(0xFF_FF_FF_FF),
    );
    mbr.overwrite_lba0(&mut disk).unwrap();

    // create new GPT structure
    let block_size = gpt::disk::LogicalBlockSize::Lb512;
    let mut gpt = gpt::GptConfig::new()
        .writable(true)
        .initialized(false)
        .logical_block_size(block_size)
        .create_from_device(Box::new(&mut disk), None).unwrap();
    gpt.update_partitions(Default::default())
        .unwrap();

    // add new EFI system partition and get its byte offset in the file
    let partition_id = gpt
        .add_partition("boot", partition_size, gpt::partition_types::EFI, 0, None)
        .unwrap();
    let partition = gpt
        .partitions()
        .get(&partition_id)
        .unwrap();
    let start_offset = partition
        .bytes_start(block_size)
        .unwrap();

    // close the GPT structure and write out changes
    gpt.write().unwrap();

    // place the FAT filesystem in the newly created partition
    disk.seek(io::SeekFrom::Start(start_offset)).unwrap();
    io::copy(
        &mut File::open(fat_image).unwrap(),
        &mut disk,
    ).unwrap();
}
