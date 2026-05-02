This project is the foundation for a professional rendering engine, similar in scope as Unity and Unreal. 

There are 2 threads: 
- main/render thread: responsible for rendering display lists and handling events
- ui thread: responsible for creating display lists

Ultimately, this project should run cross-platform: Linux, Android, Windows, MacOS, iOS and should be supported with a wide range of GPU's (opengl-es 3.0 minimum). Order of priority: OpenGL first, Vulkan next, Metal last.
