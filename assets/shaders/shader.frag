#version 450
#extension GL_ARB_separate_shader_objects : enable

// Fragment shader that uses a texture coordinate to sample from a texture
// uniform.

layout(location = 0) in vec2 textureCoord;
layout(set = 0, binding = 1) uniform texture2D backgroundTexture;
layout(set = 0, binding = 2) uniform sampler textureSampler;

layout(location = 0) out vec4 outColor;

void main() {
    outColor = texture(sampler2D(backgroundTexture, textureSampler), textureCoord);
}
