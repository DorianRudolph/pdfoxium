// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! Explicit initialization only; selecting/running a corpus never downloads it.
use crate::catalog::Case;
use crate::Result;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent()
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn install(mut body: impl Read, destination: &Path) -> Result<u64> {
    // Publishers can return an HTML challenge with status 200. Check the
    // permitted PDF-header prefix before replacing any cached local copy.
    let mut prefix = Vec::with_capacity(1024);
    body.by_ref().take(1024).read_to_end(&mut prefix)?;
    if !prefix.windows(5).any(|bytes| bytes == b"%PDF-") {
        return Err(
            "download is not a PDF (missing PDF header; possibly an HTML challenge)".into(),
        );
    }
    fs::create_dir_all(destination.parent().ok_or("download has no parent")?)?;
    let mut name = destination.as_os_str().to_os_string();
    name.push(format!(".download-{}", std::process::id()));
    let temporary = Temporary(PathBuf::from(name));
    let mut file = File::create(&temporary.0)?;
    file.write_all(&prefix)?;
    let bytes = prefix.len() as u64 + std::io::copy(&mut body, &mut file)?;
    file.flush()?;
    drop(file);
    fs::rename(&temporary.0, destination)?;
    Ok(bytes)
}

pub fn run(cases: &[Case], refresh: bool, timeout: Duration) -> Result<()> {
    let agent = agent(timeout);
    let (mut downloaded, mut reused, mut failed) = (0, 0, 0);
    for case in cases {
        let path = case.input_path();
        if path.is_file() && (!refresh || case.download_url.is_none()) {
            println!("{}: using existing {}", case.id, path.display());
            reused += 1;
            continue;
        }
        let result = (|| -> Result<u64> {
            let url = case.download_url.as_ref().ok_or(
                "missing PDF has no download URL; restore a bundled fixture or supply this local-only input yourself",
            )?;
            let mut response = agent.get(url).call()?;
            install(response.body_mut().as_reader(), &path)
        })();
        match result {
            Ok(bytes) => {
                downloaded += 1;
                println!("{}: downloaded {bytes} bytes", case.id);
            }
            Err(error) => {
                failed += 1;
                eprintln!("{}: download failed: {error}", case.id);
            }
        }
    }
    println!("Fetch: {downloaded} downloaded, {reused} reused, {failed} failed");
    if failed != 0 {
        return Err(format!("{failed} download(s) failed").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_download_keeps_old_file_and_removes_temporary_then_retry_replaces() {
        let directory =
            crate::project_root().join(format!("target/fetch-regression-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("fixture.pdf");
        fs::write(&path, b"old").unwrap();
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("truncated HTTP body"))
            }
        }
        let mut prefix = vec![b' '; 1024];
        prefix[..9].copy_from_slice(b"%PDF-1.7\n");
        assert!(install(std::io::Cursor::new(prefix).chain(Broken), &path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        assert!(install(&b""[..], &path).is_err());
        assert!(install(&b"<html>publisher challenge</html>"[..], &path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        let valid = b"%PDF-1.7\nnew PDF";
        assert_eq!(install(&valid[..], &path).unwrap(), valid.len() as u64);
        assert_eq!(fs::read(&path).unwrap(), valid);
        fs::remove_dir_all(directory).unwrap();
    }
}
