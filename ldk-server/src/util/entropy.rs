// This file is Copyright its original authors, visible in version control
// history.
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

use std::fs;
use std::io;
use std::path::Path;
use std::str::FromStr;

use ldk_node::bip39::Mnemonic;
use ldk_node::entropy::NodeEntropy;
use log::info;

use crate::util::{create_dir_all_private, read_to_string_with_limit, write_new};

const DEFAULT_MNEMONIC_FILE: &str = "keys_mnemonic";
const DEFAULT_SEED_FILE: &str = "keys_seed";
const MNEMONIC_FILE_SIZE_LIMIT: usize = 1024;

pub(crate) fn load_or_generate_node_entropy(storage_dir: &Path) -> io::Result<NodeEntropy> {
	let mnemonic_path = storage_dir.join(DEFAULT_MNEMONIC_FILE);
	let seed_path = storage_dir.join(DEFAULT_SEED_FILE);
	let seed_exists = seed_path.try_exists()?;
	let mnemonic_exists = mnemonic_path.try_exists()?;

	// A node that already started on keys_mnemonic must not switch back to a leftover
	// keys_seed. Those are different secrets, and the persisted channel state belongs
	// to whichever one was used last.
	if seed_exists && mnemonic_exists {
		return Err(io::Error::new(
			io::ErrorKind::InvalidData,
			format!(
				"Refusing to choose between {} and {}. Remove the unused secret before restarting.",
				seed_path.display(),
				mnemonic_path.display()
			),
		));
	}

	if seed_exists {
		return load_node_entropy_from_seed(&seed_path);
	}

	// Re-check immediately before creating a mnemonic. A keys_seed that appears after
	// the first check must not be ignored, or the next start would see both secrets
	// and refuse to boot.
	if seed_path.try_exists()? {
		return load_node_entropy_from_seed(&seed_path);
	}

	let mnemonic = match read_to_string_with_limit(&mnemonic_path, MNEMONIC_FILE_SIZE_LIMIT) {
		Ok(raw) => Mnemonic::from_str(raw.trim()).map_err(|e| {
			io::Error::new(
				io::ErrorKind::InvalidData,
				format!("Invalid BIP39 mnemonic in {}: {}", mnemonic_path.display(), e),
			)
		})?,
		Err(e) if e.kind() == io::ErrorKind::NotFound => {
			if let Some(parent) = mnemonic_path.parent() {
				create_dir_all_private(parent)?;
			}
			let mnemonic = Mnemonic::generate(24).map_err(io::Error::other)?;
			write_new(&mnemonic_path, format!("{}\n", mnemonic).as_bytes(), 0o600)?;
			info!(
				"Generated new BIP39 mnemonic at {}. Back up this file securely — it is required to recover on-chain funds.",
				mnemonic_path.display()
			);
			mnemonic
		},
		Err(e) => return Err(e),
	};

	Ok(NodeEntropy::from_bip39_mnemonic(mnemonic, None))
}

fn load_node_entropy_from_seed(seed_path: &Path) -> io::Result<NodeEntropy> {
	// from_seed_path generates a new 64-byte seed when the path is missing. Read first
	// so a vanished file cannot be replaced, and so a short or unreadable file stays an
	// error instead of falling through to a fresh mnemonic.
	let seed = fs::read(seed_path).map_err(|e| {
		io::Error::new(
			e.kind(),
			format!("Failed to read node entropy seed at {}: {e}", seed_path.display()),
		)
	})?;
	if seed.len() != 64 {
		return Err(io::Error::new(
			io::ErrorKind::InvalidData,
			format!(
				"Node entropy seed at {} must be exactly 64 bytes, found {}",
				seed_path.display(),
				seed.len()
			),
		));
	}
	// Length is valid and the file exists, so from_seed_path reads it instead of
	// generating a replacement.
	let seed_path_str = seed_path.to_str().ok_or_else(|| {
		io::Error::new(
			io::ErrorKind::InvalidData,
			format!("Node entropy seed path is not valid UTF-8: {}", seed_path.display()),
		)
	})?;
	let entropy = NodeEntropy::from_seed_path(seed_path_str.to_string()).map_err(|e| {
		io::Error::new(
			io::ErrorKind::InvalidData,
			format!("Failed to load node entropy from {}: {e}", seed_path.display()),
		)
	})?;
	info!(
		"Loaded node entropy from {}. Back up this file securely — it is required to recover on-chain funds.",
		seed_path.display()
	);
	Ok(entropy)
}

#[cfg(test)]
mod tests {
	use std::fs;
	use std::os::unix::fs::{MetadataExt, PermissionsExt};
	use std::path::PathBuf;

	use super::*;

	const KNOWN_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";

	fn tempdir(tag: &str) -> PathBuf {
		let dir = std::env::temp_dir().join(format!(
			"ldk-server-entropy-test-{}-{}",
			tag,
			std::process::id()
		));
		let _ = fs::remove_dir_all(&dir);
		fs::create_dir_all(&dir).unwrap();
		dir
	}

	#[test]
	fn generates_mnemonic_on_fresh_start() {
		let dir = tempdir("fresh");

		load_or_generate_node_entropy(&dir).unwrap();

		let mnemonic_path = dir.join(DEFAULT_MNEMONIC_FILE);
		assert!(mnemonic_path.exists(), "keys_mnemonic was not created");

		let perms = fs::metadata(&mnemonic_path).unwrap().permissions();
		assert_eq!(perms.mode() & 0o777, 0o600, "expected 0600 permissions");

		let content = fs::read_to_string(&mnemonic_path).unwrap();
		let word_count = content.split_whitespace().count();
		assert_eq!(word_count, 24, "expected 24-word mnemonic, got {}", word_count);

		let mtime_before = fs::metadata(&mnemonic_path).unwrap().mtime();
		load_or_generate_node_entropy(&dir).unwrap();
		let mtime_after = fs::metadata(&mnemonic_path).unwrap().mtime();
		assert_eq!(mtime_before, mtime_after, "mnemonic file was rewritten on second call");
	}

	#[test]
	fn rereads_existing_mnemonic_without_mutation() {
		let dir = tempdir("reread");
		let mnemonic_path = dir.join(DEFAULT_MNEMONIC_FILE);
		fs::write(&mnemonic_path, format!("{}\n", KNOWN_MNEMONIC)).unwrap();
		let bytes_before = fs::read(&mnemonic_path).unwrap();

		load_or_generate_node_entropy(&dir).unwrap();

		let bytes_after = fs::read(&mnemonic_path).unwrap();
		assert_eq!(bytes_before, bytes_after, "mnemonic file content changed");
	}

	#[test]
	fn loads_existing_keys_seed_without_creating_mnemonic() {
		let dir = tempdir("existing-seed");
		let seed_path = dir.join(DEFAULT_SEED_FILE);
		let seed = [0x42u8; 64];
		fs::write(&seed_path, seed).unwrap();

		load_or_generate_node_entropy(&dir).unwrap();

		assert!(!dir.join(DEFAULT_MNEMONIC_FILE).exists(), "keys_mnemonic was created");
		assert_eq!(fs::read(&seed_path).unwrap(), seed, "keys_seed was rewritten");
	}

	#[test]
	fn refuses_when_keys_seed_and_keys_mnemonic_both_exist() {
		let dir = tempdir("both-secrets");
		let seed_path = dir.join(DEFAULT_SEED_FILE);
		let mnemonic_path = dir.join(DEFAULT_MNEMONIC_FILE);
		let seed = vec![0x42u8; 64];
		fs::write(&seed_path, &seed).unwrap();
		fs::write(&mnemonic_path, format!("{KNOWN_MNEMONIC}\n")).unwrap();

		let err = load_or_generate_node_entropy(&dir).unwrap_err();

		assert_eq!(err.kind(), io::ErrorKind::InvalidData);
		assert!(err.to_string().contains("Refusing to choose"));
		assert_eq!(fs::read(&seed_path).unwrap(), seed, "keys_seed was rewritten");
		assert_eq!(
			fs::read_to_string(&mnemonic_path).unwrap().trim(),
			KNOWN_MNEMONIC,
			"keys_mnemonic was rewritten"
		);
	}

	#[test]
	fn rejects_invalid_keys_seed_without_creating_mnemonic() {
		let dir = tempdir("invalid-seed");
		let seed_path = dir.join(DEFAULT_SEED_FILE);
		fs::write(&seed_path, vec![0x42u8; 63]).unwrap();

		let err = load_or_generate_node_entropy(&dir).unwrap_err();

		assert_eq!(err.kind(), io::ErrorKind::InvalidData);
		assert!(err.to_string().contains("must be exactly 64 bytes"));
		assert!(!dir.join(DEFAULT_MNEMONIC_FILE).exists(), "keys_mnemonic was created");
		assert!(seed_path.exists(), "invalid keys_seed was removed");
	}

	#[test]
	fn rejects_invalid_mnemonic_file() {
		let dir = tempdir("invalid");
		fs::write(
			dir.join(DEFAULT_MNEMONIC_FILE),
			"these words are definitely not a valid bip39 phrase at all nope",
		)
		.unwrap();

		let err = load_or_generate_node_entropy(&dir).unwrap_err();
		assert_eq!(err.kind(), io::ErrorKind::InvalidData);
	}

	#[test]
	fn rejects_oversized_mnemonic_file() {
		let dir = tempdir("oversized");
		fs::write(dir.join(DEFAULT_MNEMONIC_FILE), vec![b'a'; MNEMONIC_FILE_SIZE_LIMIT + 1])
			.unwrap();

		let err = load_or_generate_node_entropy(&dir).unwrap_err();
		assert_eq!(err.kind(), io::ErrorKind::InvalidData);
	}
}
