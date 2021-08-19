use std::path::PathBuf;
use cc::Build;

#[cfg(feature = "pkg-config")]
fn pkg_config(probe: &str, is_static: bool) -> std::result::Result<Vec<PathBuf>, pkg_config::Error> {
    let library = pkg_config::Config::new()
        .statik(is_static)
        .cargo_metadata(!cfg!(feature = "non-cargo"))
        .probe(probe)?;
    Ok(library.include_paths)
}

#[cfg(not(feature = "pkg-config"))]
fn pkg_config(_probe: &str, _is_static: bool) -> std::result::Result<Vec<PathBuf>, std::io::Error> {
    unimplemented!()
}

fn verify_lib_dir<P: AsRef<std::path::Path>>(lib_dir: P) {
    let lib_dir = lib_dir
        .as_ref()
        .canonicalize()
        .unwrap_or_else(|_| panic!("Failed to canonicalize {:?}", lib_dir.as_ref()));
    let dir_lst = std::fs::read_dir(&lib_dir)
        .unwrap_or_else(|_| panic!("Failed to open library dir {:?}", lib_dir));
    if dir_lst.count() == 0 {
        panic!(
            "Library dir {:?} is empty and is probably incorrect. Verify it is the correct path.",
            lib_dir
        )
    }
}

fn get_lib_dir(lib_name: &str) -> String {
    let lib_var_name = format!("{}_LIB_DIR", lib_name.to_uppercase());
    match std::env::var(&lib_var_name) {
        Ok(lib_dir) => lib_dir,
        Err(_) => panic!(
            "You must set {} and it must be valid UTF-8 in order to link this library!",
            lib_var_name
        ),
    }
}

// Use `alt_name` when the project and its library have different names, such as bzip2 which has
// `libbz2` as opposed to `libbzip2`.
fn link_lib(lib_name: &str, alt_name: Option<&str>, pkg_name: Option<&str>) {
    #[cfg(feature = "static")]
    const IS_STATIC: bool = true;
    #[cfg(not(feature = "static"))]
    const IS_STATIC: bool = false;

    if !cfg!(feature = "pkg-config") || pkg_config(pkg_name.unwrap_or(lib_name), IS_STATIC).is_err() {
        let lib_dir = get_lib_dir(lib_name);
        verify_lib_dir(&lib_dir);
        println!("cargo:rustc-link-search=native={}", lib_dir);
        let link_name = alt_name.unwrap_or(lib_name);
        if !cfg!(feature = "static") {
            println!(
                "cargo:rustc-link-lib={}",
                link_name
            );
        } else {
            println!(
                "cargo:rustc-link-lib=dylib={}",
                link_name
            );
        }
    }
}

fn link_cpp() {
    let mut build = Build::new();
    let tool = build.get_compiler();
    println!("IS LIKE {} {} {}", tool.is_like_gnu(), tool.is_like_clang(), tool.is_like_msvc());
    let stdlib = if tool.is_like_gnu() {
        "libstdc++.a"
    } else if tool.is_like_clang() {
        "libc++.a"
    } else {
        // Don't link to c++ statically on windows.
        return;
    };
    let output = tool
        .to_command()
        .arg("--print-file-name")
        .arg(stdlib)
        .output()
        .unwrap();
    if !output.status.success() || output.stdout.is_empty() {
        // fallback to dynamically
        return;
    }
    let path = match std::str::from_utf8(&output.stdout) {
        Ok(path) => PathBuf::from(path),
        Err(_) => return,
    };
    if !path.is_absolute() {
        return;
    }
    // remove lib prefix and .a postfix.
    let libname = &stdlib[3..stdlib.len() - 2];
    // optional static linking
    if cfg!(feature = "static_libcpp") {
        println!("cargo:rustc-link-lib={}", &libname);
    } else {
        println!("cargo:rustc-link-lib=dylib={}", &libname);
    }
    println!(
        "cargo:rustc-link-search=native={}",
        path.parent().unwrap().display()
    );
    build.cpp_link_stdlib(None);
}

pub fn link_dependencies() {
    #[cfg(feature = "bzip2")]
    link_lib("bzip2", Some("bz2"), None);
    #[cfg(feature = "lz4")]
    link_lib("lz4", None, Some("liblz4"));
    // liblz4
    #[cfg(feature = "snappy")]
    link_lib("snappy", None, None);
    // snappy
    #[cfg(feature = "zlib")]
    link_lib("zlib", Some("z"), None);
    // zlib
    #[cfg(feature = "zstd")]
    link_lib("zstd", None, Some("libzstd"));
    // "libzstd"

    // rocksdb
    link_lib("rocksdb", None, None);

    link_cpp();
}
