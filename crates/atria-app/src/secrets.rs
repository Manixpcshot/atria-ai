//! Provider credentials in the current Windows user's Credential Manager.
//! Connection metadata stays in the webview; credentials use generic Windows
//! credentials (persisted for this user on this machine), never plaintext files.

use std::path::{Path, PathBuf};

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn legacy_dpapi_path(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("invalid connection id".into());
    }
    Ok(root.join("secrets").join(format!("{id}.dpapi")))
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::ptr::null_mut;

    const CRED_TYPE_GENERIC: u32 = 1;
    const CRED_PERSIST_LOCAL_MACHINE: u32 = 2;
    const ERROR_NOT_FOUND: u32 = 1168;
    const CRED_MAX_CREDENTIAL_BLOB_SIZE: usize = 5 * 512;

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    struct CredentialW {
        flags: u32,
        cred_type: u32,
        target_name: *mut u16,
        comment: *mut u16,
        last_written: FileTime,
        credential_blob_size: u32,
        credential_blob: *mut u8,
        persist: u32,
        attribute_count: u32,
        attributes: *mut c_void,
        target_alias: *mut u16,
        user_name: *mut u16,
    }

    #[link(name = "Advapi32")]
    extern "system" {
        fn CredWriteW(credential: *const CredentialW, flags: u32) -> i32;
        fn CredReadW(
            target: *const u16,
            cred_type: u32,
            flags: u32,
            credential: *mut *mut CredentialW,
        ) -> i32;
        fn CredDeleteW(target: *const u16, cred_type: u32, flags: u32) -> i32;
        fn CredFree(buffer: *mut c_void);
    }

    #[link(name = "Kernel32")]
    extern "system" {
        fn GetLastError() -> u32;
    }

    fn target(id: &str) -> Vec<u16> {
        format!("Atria/Provider/{id}\0").encode_utf16().collect()
    }

    pub fn set(id: &str, value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Err("credential cannot be empty".into());
        }
        let bytes = value.as_bytes();
        if bytes.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE {
            return Err("credential is larger than the Windows Credential Manager limit".into());
        }
        let mut name = target(id);
        let mut user = format!("Atria connection {id}\0").encode_utf16().collect::<Vec<_>>();
        let mut comment = "Atria provider API credential\0".encode_utf16().collect::<Vec<_>>();
        let credential = CredentialW {
            flags: 0,
            cred_type: CRED_TYPE_GENERIC,
            target_name: name.as_mut_ptr(),
            comment: comment.as_mut_ptr(),
            last_written: FileTime { low: 0, high: 0 },
            credential_blob_size: bytes.len() as u32,
            credential_blob: bytes.as_ptr() as *mut u8,
            persist: CRED_PERSIST_LOCAL_MACHINE,
            attribute_count: 0,
            attributes: null_mut(),
            target_alias: null_mut(),
            user_name: user.as_mut_ptr(),
        };
        let ok = unsafe { CredWriteW(&credential, 0) };
        if ok == 0 {
            Err(format!("Windows Credential Manager could not save the credential (error {})", unsafe { GetLastError() }))
        } else {
            Ok(())
        }
    }

    pub fn get(id: &str) -> Result<Option<String>, String> {
        let name = target(id);
        let mut credential: *mut CredentialW = null_mut();
        let ok = unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
        if ok == 0 {
            let code = unsafe { GetLastError() };
            return if code == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(format!("Windows Credential Manager could not read the credential (error {code})"))
            };
        }
        if credential.is_null() {
            return Err("Windows Credential Manager returned an empty credential".into());
        }
        let stored = unsafe { &*credential };
        let result = if stored.credential_blob_size as usize > CRED_MAX_CREDENTIAL_BLOB_SIZE {
            Err("stored credential exceeds the Windows Credential Manager limit".into())
        } else if stored.credential_blob_size == 0 {
            Err("Windows Credential Manager returned an empty credential".into())
        } else if stored.credential_blob.is_null() {
            Err("Windows Credential Manager returned an invalid credential blob".into())
        } else {
            let bytes = unsafe {
                std::slice::from_raw_parts(stored.credential_blob, stored.credential_blob_size as usize)
            };
            std::str::from_utf8(bytes)
                .map(str::to_owned)
                .map_err(|_| "stored credential is not valid UTF-8".to_string())
        };
        unsafe { CredFree(credential.cast()); }
        result.map(Some)
    }

    pub fn delete(id: &str) -> Result<(), String> {
        let name = target(id);
        let ok = unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if ok != 0 {
            return Ok(());
        }
        let code = unsafe { GetLastError() };
        if code == ERROR_NOT_FOUND {
            Ok(())
        } else {
            Err(format!("Windows Credential Manager could not delete the credential (error {code})"))
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn set(_: &str, _: &str) -> Result<(), String> {
        Err("provider credentials require Windows Credential Manager".into())
    }
    pub fn get(_: &str) -> Result<Option<String>, String> {
        Err("provider credentials require Windows Credential Manager".into())
    }
    pub fn delete(_: &str) -> Result<(), String> {
        Err("provider credentials require Windows Credential Manager".into())
    }
}

// Compatibility reader for credentials encrypted by the earlier development
// DPAPI file-store implementation. Successful reads are immediately moved into
// Credential Manager and the obsolete file is deleted.
#[cfg(windows)]
mod legacy_dpapi {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "Crypt32")]
    extern "system" {
        fn CryptUnprotectData(
            input: *const DataBlob,
            description: *mut *mut u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "Kernel32")]
    extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    pub fn unprotect(cipher: &[u8]) -> Result<Vec<u8>, String> {
        if cipher.len() > u32::MAX as usize {
            return Err("encrypted legacy credential is too large".into());
        }
        let input = DataBlob { cb_data: cipher.len() as u32, pb_data: cipher.as_ptr() as *mut u8 };
        let mut output = DataBlob { cb_data: 0, pb_data: null_mut() };
        let mut description: *mut u16 = null_mut();
        let ok = unsafe {
            CryptUnprotectData(&input, &mut description, null(), null_mut(), null_mut(), 0x1, &mut output)
        };
        if ok == 0 || output.pb_data.is_null() {
            if !description.is_null() { unsafe { LocalFree(description.cast()); } }
            return Err("Windows could not decrypt the legacy credential".into());
        }
        let plain = unsafe { std::slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec() };
        unsafe {
            LocalFree(output.pb_data.cast());
            if !description.is_null() { LocalFree(description.cast()); }
        }
        Ok(plain)
    }
}

#[cfg(not(windows))]
mod legacy_dpapi {
    pub fn unprotect(_: &[u8]) -> Result<Vec<u8>, String> {
        Err("legacy DPAPI migration is supported on Windows only".into())
    }
}

pub fn set(root: &Path, id: &str, value: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("invalid connection id".into());
    }
    platform::set(id, value)?;
    if let Ok(path) = legacy_dpapi_path(root, id) {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

pub fn get(root: &Path, id: &str) -> Result<Option<String>, String> {
    if !valid_id(id) {
        return Err("invalid connection id".into());
    }
    if let Some(value) = platform::get(id)? {
        return Ok(Some(value));
    }
    let path = legacy_dpapi_path(root, id)?;
    let cipher = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read legacy encrypted credential: {error}")),
    };
    let mut plain = legacy_dpapi::unprotect(&cipher)?;
    let value = std::str::from_utf8(&plain)
        .map_err(|_| "legacy credential is not valid UTF-8".to_string())?
        .to_owned();
    plain.fill(0);
    platform::set(id, &value)?;
    std::fs::remove_file(path).map_err(|error| format!("credential moved to Windows Credential Manager, but legacy file cleanup failed: {error}"))?;
    Ok(Some(value))
}

pub fn delete(root: &Path, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("invalid connection id".into());
    }
    platform::delete(id)?;
    if let Ok(path) = legacy_dpapi_path(root, id) {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("credential deleted, but legacy file cleanup failed: {error}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::valid_id;

    #[test]
    fn rejects_path_traversal_ids() {
        assert!(valid_id("conn_123-safe"));
        assert!(!valid_id("../secrets"));
        assert!(!valid_id(""));
        assert!(!valid_id("a/b"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_credential_manager_round_trip() {
        let id = format!("test-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        super::platform::set(&id, "atria-credential-test").unwrap();
        let value = super::platform::get(&id).unwrap();
        super::platform::delete(&id).unwrap();
        assert_eq!(value.as_deref(), Some("atria-credential-test"));
    }
}
