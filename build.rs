fn main() {
    // intel_tex_2 (BCn encoder of image_dds) ships C++ objects but does not link libstdc++.
    println!("cargo:rustc-link-lib=stdc++");
}
