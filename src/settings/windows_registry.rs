use std::ffi::{CString};
use std::ptr::{null, null_mut};
#[cfg(windows)]
use winapi::{
    shared::minwindef::{HKEY, MAX_PATH, DWORD},
    um::{
        winnt::{
            REG_SZ, REG_OPTION_NON_VOLATILE, KEY_WRITE,
        },
        winreg::{
            RegCreateKeyExA, RegSetValueExA, RegCloseKey, RegDeleteTreeA,
            HKEY_CLASSES_ROOT
        },
        libloaderapi::{
            GetModuleFileNameA
        }
    }
};

#[cfg(target_os = "windows")]
fn get_binary_path() -> String {
    let mut buffer = vec![0u8; MAX_PATH];
    unsafe {
        GetModuleFileNameA(null_mut(), buffer.as_mut_ptr() as *mut i8, MAX_PATH as DWORD);
        CString::from_vec_unchecked(buffer)
        .into_string().unwrap_or("".to_string())
        .trim_end_matches(char::from(0)).to_string()
    }
}

#[cfg(target_os = "windows")]
pub fn unregister_rightclick() {
    let str_registry_path_1 = CString::new("Directory\\Background\\shell\\Neovide").unwrap();
    let str_registry_path_2 = CString::new("*\\shell\\Neovide").unwrap();
    unsafe {
        RegDeleteTreeA(HKEY_CLASSES_ROOT, str_registry_path_1.as_ptr());
        RegDeleteTreeA(HKEY_CLASSES_ROOT, str_registry_path_2.as_ptr());
    }
}

#[cfg(target_os = "windows")]
pub fn register_rightclick_directory() {
    let neovide_path = get_binary_path();
    let mut registry_key: HKEY = null_mut();
    let str_registry_path = CString::new("Directory\\Background\\shell\\Neovide").unwrap();
    let str_registry_command_path = CString::new("Directory\\Background\\shell\\Neovide\\command").unwrap();
    let str_icon = CString::new("Icon").unwrap();
    let str_command = CString::new(format!("{} \"%V\"", neovide_path).as_bytes()).unwrap();
    let str_description= CString::new("Open with Neovide").unwrap();
    let str_neovide_path = CString::new(neovide_path.as_bytes()).unwrap();
    unsafe {
        RegCreateKeyExA(
            HKEY_CLASSES_ROOT, str_registry_path.as_ptr(),
            0, null_mut(),
            REG_OPTION_NON_VOLATILE, KEY_WRITE,
            null_mut(), &mut registry_key, null_mut()
        );
        let registry_values = [
            (null(), REG_SZ, str_description.as_ptr() as *const u8, str_description.to_bytes().len() + 1),
            (str_icon.as_ptr(), REG_SZ, str_neovide_path.as_ptr() as *const u8, str_neovide_path.to_bytes().len() + 1),
        ];
        for &(key, keytype, value_ptr, size_in_bytes) in &registry_values {
            RegSetValueExA(registry_key, key, 0, keytype, value_ptr, size_in_bytes as u32);
        }
        RegCloseKey(registry_key);

        RegCreateKeyExA(
            HKEY_CLASSES_ROOT, str_registry_command_path.as_ptr(),
            0, null_mut(),
            REG_OPTION_NON_VOLATILE, KEY_WRITE,
            null_mut(), &mut registry_key, null_mut()
        );
        let registry_values = [
            (null(), REG_SZ, str_command.as_ptr() as *const u8, str_command.to_bytes().len() + 1),
        ];
        for &(key, keytype, value_ptr, size_in_bytes) in &registry_values {
            RegSetValueExA(registry_key, key, 0, keytype, value_ptr, size_in_bytes as u32);
        }
        RegCloseKey(registry_key);
    }
}

#[cfg(target_os = "windows")]
pub fn register_rightclick_file() {
    let neovide_path = get_binary_path();
    let mut registry_key: HKEY = null_mut();
    let str_registry_path = CString::new("*\\shell\\Neovide").unwrap();
    let str_registry_command_path = CString::new("*\\shell\\Neovide\\command").unwrap();
    let str_icon = CString::new("Icon").unwrap();
    let str_command = CString::new(format!("{} \"%1\"", neovide_path).as_bytes()).unwrap();
    let str_description= CString::new("Open with Neovide").unwrap();
    let str_neovide_path = CString::new(neovide_path.as_bytes()).unwrap();
    unsafe {
        RegCreateKeyExA(
            HKEY_CLASSES_ROOT, str_registry_path.as_ptr(),
            0, null_mut(),
            REG_OPTION_NON_VOLATILE, KEY_WRITE,
            null_mut(), &mut registry_key, null_mut()
        );
        let registry_values = [
            (null(), REG_SZ, str_description.as_ptr() as *const u8, str_description.to_bytes().len() + 1),
            (str_icon.as_ptr(), REG_SZ, str_neovide_path.as_ptr() as *const u8, str_neovide_path.to_bytes().len() + 1),
        ];
        for &(key, keytype, value_ptr, size_in_bytes) in &registry_values {
            RegSetValueExA(registry_key, key, 0, keytype, value_ptr, size_in_bytes as u32);
        }
        RegCloseKey(registry_key);

        RegCreateKeyExA(
            HKEY_CLASSES_ROOT, str_registry_command_path.as_ptr(),
            0, null_mut(),
            REG_OPTION_NON_VOLATILE, KEY_WRITE,
            null_mut(), &mut registry_key, null_mut()
        );
        let registry_values = [
            (null(), REG_SZ, str_command.as_ptr() as *const u8, str_command.to_bytes().len() + 1),
        ];
        for &(key, keytype, value_ptr, size_in_bytes) in &registry_values {
            RegSetValueExA(registry_key, key, 0, keytype, value_ptr, size_in_bytes as u32);
        }
        RegCloseKey(registry_key);
    }
}

