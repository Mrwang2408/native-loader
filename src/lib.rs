use std::ffi::CString;
use std::fs;
use std::os::raw::{c_char, c_int, c_void};

extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
}

const RTLD_NOW: c_int = 0x2;
const RTLD_GLOBAL: c_int = 0x100;

/// 从 /proc/self/cmdline 拿包名（去掉 :remote 后缀）
fn pkg_name() -> String {
    fs::read("/proc/self/cmdline")
        .ok()
        .and_then(|b| {
            let end = b.iter().position(|&c| c == 0)?;
            let s = String::from_utf8_lossy(&b[..end]).to_string();
            Some(s.split(':').next()?.to_string())
        })
        .unwrap_or_default()
}

fn load_dir(dir: &str) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return, // 目录不存在/无权限，静默跳过
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with("lib") && name.ends_with(".so") {
            if let (Some(p), Ok(c)) = (path.to_str(), CString::new(path.to_str().unwrap_or(""))) {
                unsafe {
                    dlopen(c.as_ptr(), RTLD_NOW | RTLD_GLOBAL);
                }
            }
        }
    }
}

/// 库被 ld.so 加载时自动执行，无论 DT_NEEDED、dlopen 还是 System.loadLibrary
#[ctor::ctor]
fn init() {
    let pkg = pkg_name();
    // 优先级：应用私有目录（无权限问题、SELinux 允许 dlopen）
    // load_dir(&format!("/data/user/0/{pkg}/files/libs"));
    // load_dir(&format!("/data/data/{pkg}/files/libs"));
    load_dir(&format!("/data/data/{pkg}/cache/native_libs"));
    // 兜底：外部存储（Android 10+ 大概率失败，仅老版本可用）
    // load_dir("/sdcard/Download");
}