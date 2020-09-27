//! This build script compiles all shaders from `SHADER_SRC` into SPIR-V representations in
//! `SPIRV_OUT`.

use std::io::Read;

const SHADER_SRC: &str = "assets/shaders";
const SPIRV_OUT: &str = "assets/generated/spirv";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use glsl_to_spirv::ShaderType;

    println!("cargo:rerun-if-changed={}", SHADER_SRC);

    std::fs::create_dir_all(SPIRV_OUT)?;

    let shader_src_path = std::path::Path::new(SHADER_SRC);
    for shader_file in ["shader.vert", "shader.frag"].iter() {
        let shader_path = shader_src_path.join(shader_file);

        let shader_type = match shader_path
            .extension()
            .unwrap_or_else(|| panic!("Shader {:?} has no extension", shader_path))
            .to_string_lossy()
            .as_ref()
        {
            "vert" => ShaderType::Vertex,
            "frag" => ShaderType::Fragment,
            _ => panic!("Unrecognized shader type for {:?}", shader_path),
        };

        let source = std::fs::read_to_string(&shader_path)?;
        let mut compiled_file = glsl_to_spirv::compile(&source, shader_type)?;

        let mut compiled_bytes = Vec::new();
        compiled_file.read_to_end(&mut compiled_bytes)?;

        let out_path = format!(
            "{}/{}.spv",
            SPIRV_OUT,
            shader_path.file_name().unwrap().to_string_lossy()
        );

        std::fs::write(&out_path, &compiled_bytes)?;
    }

    Ok(())
}
