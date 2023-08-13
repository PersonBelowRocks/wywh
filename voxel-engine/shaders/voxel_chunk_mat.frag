#version 450

layout(location = 0) out vec4 o_Target;

layout(location = 0) in vec3 normal;

void main() {
    o_Target = vec4(0.8, 0.4, 0.8, 1.0);
}