use std::env;

fn link(name: &str, bundled: bool) {
    use std::env::var;
    let target = var("TARGET").unwrap();
    let target: Vec<_> = target.split('-').collect();
    if target.get(2) == Some(&"windows") {
        println!("cargo:rustc-link-lib=dylib={}", name);
        if bundled && target.get(3) == Some(&"gnu") {
            let dir = var("CARGO_MANIFEST_DIR").unwrap();
            println!("cargo:rustc-link-search=native={}/{}", dir, target[0]);
        }
    }
}

#[cfg(any(
    feature = "bzip2",
    feature = "lz4",
    feature = "snappy",
    feature = "zlib",
    feature = "zstd"
))]
fn enforce_rerun<P: AsRef<std::path::Path>>(path: P) {
    println!("cargo:rerun-if-changed={}", path.as_ref().to_string_lossy());
}

#[cfg(any(
    feature = "bzip2",
    feature = "lz4",
    feature = "snappy",
    feature = "zlib",
    feature = "zstd"
))]
fn check_submodule<P: AsRef<std::path::Path>>(path: P) {
    let path = path
        .as_ref()
        .canonicalize()
        .unwrap_or_else(|_| panic!("Failed to canonicalize {:?}", path.as_ref()));
    let dir =
        std::fs::read_dir(&path).unwrap_or_else(|_| panic!("Failed to open directory {:?}", path));
    if dir.count() == 0 {
        eprintln!(
            "The `{:?}` directory is empty, did you forget to pull the submodules?",
            path
        );
        eprintln!("Try `git submodule update --init --recursive`");
        panic!(
            "Missing submodule {}",
            path.file_name().unwrap().to_string_lossy()
        )
    }
}

fn build_rocksdb() {
    let target = env::var("TARGET").unwrap();
    let mut build = cc::Build::new();

    build
        .include("./rocksdb/include/")
        .include("./rocksdb/")
        .include("./rocksdb/third-party/gtest-1.8.1/fused-src/");

    #[cfg(feature = "snappy")]
    {
        build.define("SNAPPY", Some("1"));
        build.include("./snappy/");

    }

    #[cfg(feature = "lz4")]
    {
        build.define("LZ4", Some("1"));
        build.include("./lz4/lib/");
    }

    #[cfg(feature = "zstd")]
    {
        build.define("ZSTD", Some("1"));
        build
            .include("./zstd/lib/")
            .include("./zstd/lib/dictBuilder/");
    }

    #[cfg(feature = "zlib")]
    {
        build.define("ZLIB", Some("1"));
        build.include("./zlib/");
    }

    #[cfg(feature = "bzip2")]
    {
        build.define("BZIP2", Some("1"));
        build.include("./bzip2/");
    }

    build.include(".");
    build.define("NDEBUG", Some("1"));

    let mut lib_sources = include_str!("../rocksdb_lib_sources.txt")
        .trim()
        .split("\n")
        .map(str::trim)
        .collect::<Vec<&'static str>>();

    // We have a pregenerated a version of build_version.cc in the local directory
    lib_sources = lib_sources
        .iter()
        .cloned()
        .filter(|file| *file != "util/build_version.cc")
        .collect::<Vec<&'static str>>();

    if target.contains("x86_64") {
        // This is needed to enable hardware CRC32C. Technically, SSE 4.2 is
        // only available since Intel Nehalem (about 2010) and AMD Bulldozer
        // (about 2011).
        let target_feature = env::var("CARGO_CFG_TARGET_FEATURE").unwrap();
        let target_features: Vec<_> = target_feature.split(",").collect();
        if target_features.contains(&"sse2") {
            build.flag_if_supported("-msse2");
        }
        if target_features.contains(&"sse4.1") {
            build.flag_if_supported("-msse4.1");
        }
        if target_features.contains(&"sse4.2") {
            build.flag_if_supported("-msse4.2");
            build.define("HAVE_SSE42", Some("1"));
        }

        if !target.contains("android") {
            if target_features.contains(&"pclmulqdq") {
                build.define("HAVE_PCLMUL", Some("1"));
                build.flag_if_supported("-mpclmul");
            }
        }
    }

    if target.contains("aarch64") {
        lib_sources.push("util/crc32c_arm64.cc")
    }

    if target.contains("darwin") {
        build
            .define("OS_MACOSX", Some("1"))
            .define("ROCKSDB_PLATFORM_POSIX", Some("1"))
            .define("ROCKSDB_LIB_IO_POSIX", Some("1"));
    } else if target.contains("android") {
        build
            .define("OS_ANDROID", Some("1"))
            .define("ROCKSDB_PLATFORM_POSIX", Some("1"))
            .define("ROCKSDB_LIB_IO_POSIX", Some("1"));
    } else if target.contains("linux") {
        build
            .define("OS_LINUX", Some("1"))
            .define("ROCKSDB_PLATFORM_POSIX", Some("1"))
            .define("ROCKSDB_LIB_IO_POSIX", Some("1"));
    } else if target.contains("freebsd") {
        build
            .define("OS_FREEBSD", Some("1"))
            .define("ROCKSDB_PLATFORM_POSIX", Some("1"))
            .define("ROCKSDB_LIB_IO_POSIX", Some("1"));
    } else if target.contains("windows") {
        link("rpcrt4", false);
        link("shlwapi", false);
        build.define("DWIN32", None);
        build.define("OS_WIN", None);
        build.define("_MBCS", None);
        build.define("WIN64", None);
        build.define("NOMINMAX", None);
        build
            .define("OS_WIN", Some("1"))
            .define("ROCKSDB_WINDOWS_UTF8_FILENAMES", Some("1"));

        if &target == "x86_64-pc-windows-gnu" {
            // Tell MinGW to create localtime_r wrapper of localtime_s function.
            build.define("_POSIX_C_SOURCE", Some("1"));
            // Tell MinGW to use at least Windows Vista headers instead of the ones of Windows XP.
            // (This is minimum supported version of rocksdb)
            build.define("_WIN32_WINNT", Some("_WIN32_WINNT_VISTA"));
        }

        // Remove POSIX-specific sources
        lib_sources = lib_sources
            .iter()
            .cloned()
            .filter(|file| {
                !matches!(
                    *file,
                    "port/port_posix.cc"
                        | "env/env_posix.cc"
                        | "env/fs_posix.cc"
                        | "env/io_posix.cc"
                )
            })
            .collect::<Vec<&'static str>>();

        // Add Windows-specific sources
        lib_sources.push("port/win/port_win.cc");
        lib_sources.push("port/win/env_win.cc");
        lib_sources.push("port/win/env_default.cc");
        lib_sources.push("port/win/win_logger.cc");
        lib_sources.push("port/win/io_win.cc");
        lib_sources.push("port/win/win_thread.cc");
    }

    if target.contains("msvc") {
        build.flag("-EHsc");
    } else {
        build.flag("-std=c++11");
        // this was breaking the build on travis due to
        // > 4mb of warnings emitted.
        build.flag("-Wno-unused-parameter");
    }

    build.define("ROCKSDB_SUPPORT_THREAD_LOCAL", None);

    if cfg!(feature = "jemalloc") {
        build.define("WITH_JEMALLOC", "ON");
    }

    if target.contains("msvc") {
        build.flag("-EHsc");
    } else {
        build.flag(&cxx_standard());
        // this was breaking the build on travis due to
        // > 4mb of warnings emitted.
        build.flag("-Wno-unused-parameter");
    }

    for file in lib_sources {
        let file = "rocksdb/".to_string() + file;
        build.file(&file);
    }

    build.file("build_version.cc");

    build.cpp(true);
    build.compile("librocksdb.a");
}

#[cfg(feature = "snappy")]
fn build_snappy() {
    enforce_rerun("./snappy");
    check_submodule("./snappy");
    let target = env::var("TARGET").expect("No TARGET in environment");
    let endianness = env::var("CARGO_CFG_TARGET_ENDIAN").unwrap();
    let mut build = cc::Build::new();

    build.include("./snappy/").include("./");
    build.define("NDEBUG", Some("1"));
    build.extra_warnings(false);

    if target.contains("msvc") {
        build.flag("-EHsc");
    } else {
        build.flag("-std=c++11");
    }

    if endianness == "big" {
        build.define("SNAPPY_IS_BIG_ENDIAN", Some("1"));
    }

    build
        .file("./snappy/snappy.cc")
        .file("./snappy/snappy-sinksource.cc")
        .file("./snappy/snappy-c.cc");
    build.cpp(true);
    build.compile("libsnappy.a");
}

#[cfg(feature = "lz4")]
fn build_lz4() {
    enforce_rerun("./lz4");
    check_submodule("./lz4");

    let mut build = cc::Build::new();

    build.opt_level(3);

    let target = env::var("TARGET").unwrap();

    if target.contains("i686-pc-windows-gnu") {
        build.flag("-fno-tree-vectorize");
    }

    build
        .file("./lz4/lib/lz4.c")
        .file("./lz4/lib/lz4frame.c")
        .file("./lz4/lib/lz4hc.c")
        .file("./lz4/lib/xxhash.c");

    build.compile("liblz4.a");
}

#[cfg(feature = "zstd")]
fn build_zstd() {
    enforce_rerun("./zstd");
    check_submodule("./zstd");
    let mut build = cc::Build::new();

    build
        .include("./zstd/lib/")
        .include("./zstd/lib/common/")
        .include("./zstd/lib/legacy/");

    build.define("ZSTD_LIB_DEPRECATED", Some("0"));

    build.opt_level(3);
    build.extra_warnings(false);

    let globs = &[
        "./zstd/lib/common/*.c",
        "./zstd/lib/compress/*.c",
        "./zstd/lib/decompress/*.c",
        "./zstd/lib/dictBuilder/*.c",
        "./zstd/lib/legacy/*.c",
    ];
    globs
        .iter()
        .map(|pattern| glob::glob(pattern).unwrap())
        .flatten()
        .map(|p| p.unwrap())
        .fold(&mut build, cc::Build::file);

    build.compile("libzstd.a");
}

#[cfg(feature = "zlib")]
fn build_zlib() {
    enforce_rerun("./zlib");
    check_submodule("./zlib");

    let mut build = cc::Build::new();

    build.opt_level(3);
    build.flag_if_supported("-Wno-implicit-function-declaration");
    build.extra_warnings(false);

    let globs = &["./zlib/*.c"];
    globs
        .iter()
        .map(|pattern| glob::glob(pattern).unwrap())
        .flatten()
        .map(|p| p.unwrap())
        .fold(&mut build, cc::Build::file);

    build.compile("libz.a");
}

#[cfg(feature = "bzip2")]
fn build_bzip2() {
    enforce_rerun("./bzip2");
    check_submodule("./bzip2");
    let mut build = cc::Build::new();

    build.extra_warnings(false);
    build.opt_level(3);
    build.extra_warnings(false);

    build
        .define("_FILE_OFFSET_BITS", Some("64"))
        .define("BZ_NO_STDIO", None);

    build
        .file("./bzip2/blocksort.c")
        .file("./bzip2/bzlib.c")
        .file("./bzip2/compress.c")
        .file("./bzip2/crctable.c")
        .file("./bzip2/decompress.c")
        .file("./bzip2/huffman.c")
        .file("./bzip2/randtable.c");

    build.compile("libbz2.a");
}

fn cxx_standard() -> String {
    env::var("ROCKSDB_CXX_STD").map_or("-std=c++11".to_owned(), |cxx_std| {
        if !cxx_std.starts_with("-std=") {
            format!("-std={}", cxx_std)
        } else {
            cxx_std
        }
    })
}

pub fn vendor_dependencies() {
    #[cfg(feature = "bzip2")]
    build_bzip2();

    #[cfg(feature = "lz4")]
    build_lz4();

    #[cfg(feature = "snappy")]
    build_snappy();

    #[cfg(feature = "zlib")]
    build_zlib();

    #[cfg(feature = "zstd")]
    build_zstd();

    build_rocksdb();
}
