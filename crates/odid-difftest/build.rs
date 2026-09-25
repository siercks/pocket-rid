fn main() {
    let src = "opendroneid-core-c/libopendroneid/opendroneid.c";
    println!("cargo:rerun-if-changed={src}");
    let mut b = cc::Build::new();
    b.file(src)
        .define("ODID_DISABLE_PRINTF", None)
        .warnings(false);
    // The library's packed bitfield structs need GCC/Clang syntax; MSVC cannot compile them.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        b.compiler("clang");
    }
    b.compile("opendroneid");
}
