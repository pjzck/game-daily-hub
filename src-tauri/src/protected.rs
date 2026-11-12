#[cfg(windows)]
mod windows {
    use std::{ptr, slice};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };

    pub fn transform(input: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
        let length = u32::try_from(input.len()).map_err(|_| "凭据数据过长。")?;
        let source = CRYPT_INTEGER_BLOB {
            cbData: length,
            pbData: input.as_ptr().cast_mut(),
        };
        let mut destination = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };
        // DPAPI reads the input blob and allocates the output with LocalAlloc.
        let result = unsafe {
            if encrypt {
                CryptProtectData(
                    &source,
                    ptr::null(),
                    ptr::null(),
                    ptr::null(),
                    ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut destination,
                )
            } else {
                CryptUnprotectData(
                    &source,
                    ptr::null_mut(),
                    ptr::null(),
                    ptr::null(),
                    ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut destination,
                )
            }
        };
        if result == 0 || destination.pbData.is_null() {
            return Err(if encrypt {
                "Windows 无法保护凭据。".into()
            } else {
                "Windows 无法解密凭据，请更新连接。".into()
            });
        }
        // The successful call owns this allocation until LocalFree below.
        let bytes = unsafe {
            let data =
                slice::from_raw_parts(destination.pbData, destination.cbData as usize).to_vec();
            LocalFree(destination.pbData.cast());
            data
        };
        Ok(bytes)
    }
}

pub fn transform(input: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    return windows::transform(input, encrypt);

    #[cfg(not(windows))]
    {
        let _ = (input, encrypt);
        Err("当前原型需要 Windows 凭据保护。".into())
    }
}
