use std::path::{
    Path,
    PathBuf,
};

use fig_util::directories;

pub fn backup_file(path: impl AsRef<Path>, backup_dir: Option<impl Into<PathBuf>>) -> std::io::Result<()> {
    let pathref = path.as_ref();
    if pathref.exists() {
        let name = pathref.file_name().ok_or_else(|| std::io::Error::other("Backup path has no file name"))?;
        let dir = match backup_dir {
            Some(dir) => dir.into(),
            None => directories::utc_backup_dir().map_err(std::io::Error::other)?,
        };
        let mut source = std::fs::File::open(pathref)?;
        std::fs::create_dir_all(&dir)?;
        // A repeated install/uninstall in the same timestamp must not destroy an earlier backup.
        let mut index = 0;
        loop {
            let destination = if index == 0 {
                dir.join(name)
            } else {
                dir.join(format!("{}.{}", name.to_string_lossy(), index))
            };
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&destination) {
                Ok(mut backup) => {
                    backup.set_permissions(source.metadata()?.permissions())?;
                    std::io::copy(&mut source, &mut backup)?;
                    backup.sync_all()?;
                    break;
                },
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => index += 1,
                Err(err) => return Err(err),
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_backups_preserve_originals() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join(".zshrc");
        let backups = root.path().join("backups");
        std::fs::write(&profile, b"original\r\n").unwrap();
        backup_file(&profile, Some(&backups)).unwrap();
        std::fs::write(&profile, b"installed\n").unwrap();
        backup_file(&profile, Some(&backups)).unwrap();
        assert_eq!(std::fs::read(backups.join(".zshrc")).unwrap(), b"original\r\n");
        assert_eq!(std::fs::read(backups.join(".zshrc.1")).unwrap(), b"installed\n");
    }
}
