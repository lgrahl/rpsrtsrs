(function() {var implementors = {};
implementors["float"] = [];
implementors["graphics"] = [];
implementors["opengl_graphics"] = [];
implementors["sdl2_window"] = [];

            if (window.register_implementors) {
                window.register_implementors(implementors);
            } else {
                window.pending_implementors = implementors;
            }
        
})()
