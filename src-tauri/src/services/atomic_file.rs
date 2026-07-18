//! 崩溃安全的同目录原子文件提交。

use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
    thread,
    time::Duration,
};

const PERSIST_RETRY_LIMIT: usize = 12;
const PERSIST_RETRY_BASE_DELAY: Duration = Duration::from_millis(5);

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn persist_synced(
    mut temporary: tempfile::NamedTempFile,
    destination: &Path,
    _parent: &Path,
) -> io::Result<()> {
    for attempt in 0..PERSIST_RETRY_LIMIT {
        match temporary.persist(destination) {
            Ok(_) => break,
            Err(error) => {
                let retryable = matches!(
                    error.error.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::WouldBlock
                ) || matches!(error.error.raw_os_error(), Some(5 | 32 | 33));

                if !retryable || attempt + 1 == PERSIST_RETRY_LIMIT {
                    return Err(error.error);
                }

                temporary = error.file;
                let delay = PERSIST_RETRY_BASE_DELAY * (attempt as u32 + 1);
                thread::sleep(delay);
            }
        }
    }

    // POSIX 上还需同步目录项，才能保证 rename 在掉电后持久。
    #[cfg(unix)]
    File::open(_parent)?.sync_all()?;

    Ok(())
}

/// 写入同目录临时文件，刷新内容后原子替换目标。
pub fn write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = parent_dir(path);
    fs::create_dir_all(parent)?;

    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    persist_synced(temporary, path, parent)
}

/// 将文件复制到同目录临时文件，刷新后再原子发布目标备份。
pub fn copy(source: &Path, destination: &Path) -> io::Result<u64> {
    let parent = parent_dir(destination);
    fs::create_dir_all(parent)?;

    let mut source_file = File::open(source)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    let copied = io::copy(&mut source_file, &mut temporary)?;
    temporary.as_file().sync_all()?;
    persist_synced(temporary, destination, parent)?;
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_replaces_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");

        write(&path, br#"{"version":1}"#).unwrap();
        write(&path, br#"{"version":2}"#).unwrap();

        assert_eq!(fs::read(&path).unwrap(), br#"{"version":2}"#);
    }

    #[test]
    fn atomic_copy_publishes_complete_backup() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source.json");
        let destination = dir.path().join("backups").join("copy.json");
        let contents = vec![b'x'; 128 * 1024];
        fs::write(&source, &contents).unwrap();

        assert_eq!(copy(&source, &destination).unwrap(), contents.len() as u64);
        assert_eq!(fs::read(destination).unwrap(), contents);
    }
}
