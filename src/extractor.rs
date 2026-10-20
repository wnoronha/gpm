use crate::errors::{GpmError, Result};
use flate2::read::GzDecoder;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive;
use zip::ZipArchive;

#[cfg_attr(test, mockall::automock)]
pub trait Extractor: Send + Sync {
    fn extract(&self, archive_path: &Path, dest: &Path) -> Result<()>;
    fn find_binaries(&self, dir: &Path) -> Result<Vec<PathBuf>>;
    fn is_executable(&self, path: &Path) -> Result<bool>;
}

#[derive(Default)]
pub struct ArchiveExtractor;

impl ArchiveExtractor {
    pub fn new() -> Self {
        Self
    }

    fn extract_zip(&self, archive_path: &Path, dest: &Path) -> Result<()> {
        let file = fs::File::open(archive_path)?;
        let mut archive = ZipArchive::new(file).map_err(|e| GpmError::Unknown(e.to_string()))?;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| GpmError::Unknown(e.to_string()))?;
            let outpath = match file.enclosed_name() {
                Some(path) => dest.join(path),
                None => continue,
            };

            if (*file.name()).ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    fs::create_dir_all(p)?;
                }
                let mut outfile = fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
                }
            }
        }
        Ok(())
    }

    fn extract_tar_gz(&self, archive_path: &Path, dest: &Path) -> Result<()> {
        let tar_gz = fs::File::open(archive_path)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        archive.unpack(dest)?;
        Ok(())
    }

    fn extract_tar_zst(&self, archive_path: &Path, dest: &Path) -> Result<()> {
        let tar_zst = fs::File::open(archive_path)?;
        let tar = zstd::stream::read::Decoder::new(tar_zst)?;
        let mut archive = Archive::new(tar);
        archive.unpack(dest)?;
        Ok(())
    }
}

impl Extractor for ArchiveExtractor {
    fn extract(&self, archive_path: &Path, dest: &Path) -> Result<()> {
        let extension = archive_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let filename = archive_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
            self.extract_tar_gz(archive_path, dest)
        } else if filename.ends_with(".tar.zst") {
            self.extract_tar_zst(archive_path, dest)
        } else if extension == "zip" {
            self.extract_zip(archive_path, dest)
        } else {
            Err(GpmError::Unknown(format!(
                "Unsupported archive format: {}",
                filename
            )))
        }
    }

    fn find_binaries(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        let mut binaries = Vec::new();
        if !dir.exists() {
            return Ok(binaries);
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                binaries.extend(self.find_binaries(&path)?);
            } else if self.is_executable(&path)? {
                binaries.push(path);
            }
        }
        Ok(binaries)
    }

    fn is_executable(&self, path: &Path) -> Result<bool> {
        if !path.is_file() {
            return Ok(false);
        }

        let mut file = fs::File::open(path)?;
        let mut buffer = [0; 4];
        let bytes_read = file.read(&mut buffer)?;

        if bytes_read < 4 {
            return Ok(false);
        }

        // ELF: \x7fELF
        // Mach-O: \xfe\xed\xfa\xce, \xcf\xfa\xed\xfe, \xca\xfe\xba\xbe
        // PE: MZ
        let is_binary = (buffer[0] == 0x7f && &buffer[1..4] == b"ELF")
            || (&buffer[0..4] == b"\xfe\xed\xfa\xce")
            || (&buffer[0..4] == b"\xcf\xfa\xed\xfe")
            || (&buffer[0..4] == b"\xca\xfe\xba\xbe")
            || (&buffer[0..2] == b"MZ");

        if is_binary {
            return Ok(true);
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(path)?;
            if metadata.permissions().mode() & 0o111 != 0 {
                return Ok(true);
            }
        }

        #[cfg(windows)]
        {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext = ext.to_lowercase();
                if ext == "exe" || ext == "bat" || ext == "cmd" || ext == "ps1" {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_magic_bytes_elf() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("test_elf");
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"\x7fELFsomebinarycontent").unwrap();

        let extractor = ArchiveExtractor::new();
        assert!(extractor.is_executable(&path).unwrap());
    }

    #[test]
    fn test_magic_bytes_not_binary() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("test.txt");
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"not a binary").unwrap();

        let extractor = ArchiveExtractor::new();
        assert!(!extractor.is_executable(&path).unwrap());
    }

    #[test]
    fn test_extract_tar_gz() {
        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("test.tar.gz");

        let tar_gz = fs::File::create(&archive_path).unwrap();
        let enc = flate2::write::GzEncoder::new(tar_gz, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);

        let mut header = tar::Header::new_gnu();
        header.set_size(11);
        header.set_cksum();
        tar.append_data(&mut header, "nested/file.txt", &b"hello world"[..])
            .unwrap();
        tar.into_inner().unwrap().finish().unwrap();

        let dest = temp.path().join("dest");
        fs::create_dir(&dest).unwrap();

        let extractor = ArchiveExtractor::new();
        extractor.extract(&archive_path, &dest).unwrap();

        let extracted_file = dest.join("nested").join("file.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read_to_string(&extracted_file).unwrap(), "hello world");
    }

    #[test]
    fn test_extract_zip() {
        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("test.zip");

        let file = fs::File::create(&archive_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);

        #[allow(unused_mut)]
        let mut options = zip::write::SimpleFileOptions::default();
        #[cfg(unix)]
        {
            options = options.unix_permissions(0o755);
        }
        zip.start_file("nested/file.txt", options).unwrap();
        zip.write_all(b"hello world").unwrap();
        zip.finish().unwrap();

        let dest = temp.path().join("dest");
        fs::create_dir(&dest).unwrap();

        let extractor = ArchiveExtractor::new();
        extractor.extract(&archive_path, &dest).unwrap();

        let extracted_file = dest.join("nested").join("file.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read_to_string(&extracted_file).unwrap(), "hello world");
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::metadata(&extracted_file).unwrap().permissions();
            assert_eq!(perms.mode() & 0o777, 0o755);
        }
    }

    #[test]
    fn test_extract_tar_zst() {
        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("test.tar.zst");

        let tar_zst = fs::File::create(&archive_path).unwrap();
        let enc = zstd::stream::write::Encoder::new(tar_zst, 0)
            .unwrap()
            .auto_finish();
        let mut tar = tar::Builder::new(enc);

        let mut header = tar::Header::new_gnu();
        header.set_size(11);
        header.set_cksum();
        tar.append_data(&mut header, "file.txt", &b"hello world"[..])
            .unwrap();
        drop(tar);

        let dest = temp.path().join("dest");
        fs::create_dir(&dest).unwrap();

        let extractor = ArchiveExtractor::new();
        extractor.extract(&archive_path, &dest).unwrap();

        let extracted_file = dest.join("file.txt");
        assert!(extracted_file.exists());
        assert_eq!(fs::read_to_string(&extracted_file).unwrap(), "hello world");
    }

    #[test]
    fn test_extract_unsupported() {
        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("test.rar");
        fs::File::create(&archive_path).unwrap();

        let dest = temp.path().join("dest");
        fs::create_dir(&dest).unwrap();

        let extractor = ArchiveExtractor::new();
        let res = extractor.extract(&archive_path, &dest);
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .to_string()
            .contains("Unsupported archive format"));
    }

    #[test]
    fn test_find_binaries_mixed() {
        let temp = tempdir().unwrap();

        let bin1 = temp.path().join("bin1");
        let mut f1 = fs::File::create(&bin1).unwrap();
        f1.write_all(b"\x7fELFbin1").unwrap();

        let txt1 = temp.path().join("txt1.txt");
        let mut f2 = fs::File::create(&txt1).unwrap();
        f2.write_all(b"not a binary").unwrap();

        let sub = temp.path().join("sub");
        fs::create_dir(&sub).unwrap();

        let bin2 = sub.join("bin2");
        let mut f3 = fs::File::create(&bin2).unwrap();
        f3.write_all(b"\x7fELFbin2").unwrap();

        let extractor = ArchiveExtractor::new();
        let mut binaries = extractor.find_binaries(temp.path()).unwrap();
        binaries.sort();

        let mut expected = vec![bin1, bin2];
        expected.sort();
        
        assert_eq!(binaries.len(), 2);
        assert_eq!(binaries, expected);
    }
}
