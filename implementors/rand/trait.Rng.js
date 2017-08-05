(function() {var implementors = {};
implementors["arrayvec"] = [];
implementors["enum_primitive"] = [];
implementors["gl"] = [];
implementors["graphics"] = [];
implementors["image"] = [];
implementors["libc"] = [];
implementors["num"] = [];
implementors["opengl_graphics"] = [];
implementors["rand"] = [];
implementors["rayon"] = [];
implementors["regex_syntax"] = [];
implementors["sdl2"] = [];
implementors["sdl2_window"] = [];
implementors["serde"] = [];
implementors["syn"] = [];
implementors["thread_local"] = [];

            if (window.register_implementors) {
                window.register_implementors(implementors);
            } else {
                window.pending_implementors = implementors;
            }
        
})()
