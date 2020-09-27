#version 450
#extension GL_ARB_separate_shader_objects : enable

// Vertex shader that applies a uniform matrix transformation to the position
// and directly copies the input texture coordinate to the following fragment
// shader.

layout(location = 0) in vec2 position2D;
layout(location = 1) in vec2 textureCoordInput;

layout(location = 0) out vec2 textureCoordOutput;

layout(set = 0, binding = 0) uniform Transform {
    mat4 transform;
};

out gl_PerVertex {
    vec4 gl_Position;
};

void main() {
    textureCoordOutput = textureCoordInput;

    gl_Position = transform * vec4(position2D, 0.0, 1.0);
}
