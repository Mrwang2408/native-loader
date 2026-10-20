use std::{
    ffi::{CStr, CString},
    fs,
    os::raw::{c_char, c_int, c_void},
    path::{Path, PathBuf},
    sync::{OnceLock, LazyLock, Mutex},
};

use jni::{
    objects::{JObject, JValue},
    sys::{jint, JNI_VERSION_1_6},
    JNIEnv, JavaVM,
};
use log::{error, info, warn};

type JNI_OnLoadFn = unsafe extern "C" fn(*mut jni::sys::JavaVM, *mut c_void) -> jint;

extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *mut c_char;
}

const RTLD_NOW: c_int = 0x2;
const RTLD_GLOBAL: c_int = 0x100;

const JNI_ERR: i32 = -1;

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

pub fn setup_logging() {
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("LibLoader")
            .with_max_level(log::LevelFilter::Debug),
    );
}

fn get_pkg_name() -> Option<String> {
    let data = fs::read("/proc/self/cmdline").ok()?;
    let name = data.split(|&b| b == 0 || b == b':').next()?;
    if name.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(name).into_owned())
}

fn show_toast(vm: &JavaVM, msg: &str) {
    let _ = (|| -> jni::errors::Result<()> {
        let mut env = vm.get_env().unwrap();
        let context = env
            .call_static_method(
                "android/app/ActivityThread",
                "currentApplication",
                "()Landroid/app/Application;",
                &[],
            )?
            .l()?;
        let text = env.new_string(msg)?;
        let toast = env
            .call_static_method(
                "android/widget/Toast",
                "makeText",
                "(Landroid/content/Context;Ljava/lang/CharSequence;I)Landroid/widget/Toast;",
                &[(&context).into(), (&text).into(), JValue::Int(0)],
            )?
            .l()?;
        env.call_method(toast, "show", "()V", &[])?;
        Ok(())
    })();
}

fn scan_and_load(vm: &JavaVM) {
    // let mut env = vm.get_env().unwrap();
    let Some(pkg) = get_pkg_name() else {
        error!("Cannot determine package name");
        return;
    };
    let dir = format!("/data/data/{pkg}/cache/native_libs");
    if let Err(e) = fs::create_dir_all(&dir) {
        error!("create_dir_all {dir:?} failed: {e}");
        return;
    }
    info!("Scanning: {dir:?}");
    let mut loaded = 0;
    let mut failed = 0;
    let preloader = Path::new(&dir).join("libpreloader.so");
    if preloader.exists() {
        if unsafe { load_one(&preloader, vm) } {
            loaded += 1;
        }
    }
    for entry in fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy();
        if !(path.is_file() && name.starts_with("lib") && name.ends_with(".so")) {
            continue;
        }
        if name == "libpreloader.so" {
            continue;
        }
        if unsafe { load_one(&path, vm) } {
            loaded += 1;
        } else {
            failed += 1;
        }
    }
    if failed > 0 {
        // show_toast(vm, &format!("ERROR: {failed} lib load failed"));
    }
    if loaded > 0 {
        // show_toast(vm, &format!("Loaded {loaded} libs"));
    }
    info!("LibLoader end");
}

unsafe fn load_one(path: &Path, vm: &JavaVM) -> bool {
    let name = path.file_name().unwrap().to_string_lossy();
    let c_path = CString::new(path.to_string_lossy().as_bytes()).unwrap();
    let handle = dlopen(c_path.as_ptr(), RTLD_NOW | RTLD_GLOBAL);
    if handle.is_null() {
        let raw = dlerror();
        let msg = if raw.is_null() {
            "unknown".into()
        } else {
            CStr::from_ptr(raw).to_string_lossy().into_owned()
        };
        error!("Failed to load: {msg}");
        show_toast(&vm, &format!("{msg}"));
        return false;
    }
    let sym = dlsym(handle, c"JNI_OnLoad".as_ptr());
    if sym.is_null() {
        warn!("No JNI_OnLoad in {path:?} (loaded only)");
        show_toast(&vm, &format!("load only: {name}"));
        return true;
    }
    let on_load: JNI_OnLoadFn = std::mem::transmute(sym);
    let ver = on_load(vm.get_java_vm_pointer(), std::ptr::null_mut());
    if ver == JNI_ERR {
        error!("JNI_OnLoad of {path:?} returned JNI_ERR");
        return false;
    }
    if ver != JNI_VERSION_1_6 {
        warn!("JNI_OnLoad of {path:?} returned {ver:#x}, ignore");
    }
    info!("Loaded {path:?}");
    show_toast(&vm, &format!("Loaded {name}"));
    true
}


#[no_mangle]
pub extern "C" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
    setup_logging();
    // let vm = unsafe { JavaVM::from_raw(vm) }.expect("invalid JavaVM");
    scan_and_load(&vm);
    JNI_VERSION_1_6
}

#[ctor::ctor]
fn init() {
    //setup_logging();
    //scan_and_load(&vm);
}