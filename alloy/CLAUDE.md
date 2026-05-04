This project is the foundation for a professional rendering engine, similar in scope as Unity and Unreal. 

There are 2 threads: 
- ui thread: responsible for creating display lists
- main/render thread: responsible for rendering display lists

Ultimately, this project should run cross-platform: Linux, Android, Windows, MacOS, iOS and should support a wide range of GPU's (Opengl-ES 3.0 minimum). Order of priority: OpenGL first, Vulkan next, Metal last.

