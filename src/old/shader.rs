
    const VERTEX_SHADER: &'static str = r#"#version 100
    attribute vec3 position;
    attribute vec2 texcoord;

    varying lowp vec2 uv;

    uniform mat4 Model;
    uniform mat4 Projection;

    void main() {
        gl_Position = Projection * Model * vec4(position, 1);
        uv = texcoord;
    }
    "#;

    const FRAGMENT_SHADER: &'static str = r#"#version 100
    precision mediump float;

    varying lowp vec2 uv;

    uniform sampler2D Texture;

    uniform vec2 Resolution;
    const float Radius = 40.0;
    const float Sigma = 20.0;
    // uniform vec2 Direction;

    // hard-coded directions (8 around the circle)
    const int DIRS = 16;
    const int SAMPLES = 32;
    const float TWO_PI = 6.28318530718;

    // void main() {
    //     vec4 color = texture2D(Texture, uv);
    //     float brightness = color.r + color.g + color.b;
    //     if (brightness > 0.01) {
    //         gl_FragColor = color;
    //         return;
    //     }

    //     // one texel in UV
    //     vec2 texel = 1.0 / Resolution;
        
    //     // start with the center pixel
    //     vec4 sum = texture2D(Texture, uv);
    //     float count = 1.0;
        
    //     // for each of DIRS directions…
    //     for (int d = 0; d < DIRS; ++d) {
    //         // compute the unit vector for this direction
    //         float angle = TWO_PI * float(d) / float(DIRS);
    //         vec2 dir = vec2(cos(angle), sin(angle));
            
    //         // take SAMPLES samples along that ray
    //         for (int i = 1; i <= SAMPLES; ++i) {
    //             float f = float(i) / float(SAMPLES);
    //             vec2 offs = dir * texel * Radius * f;
    //             sum   += texture2D(Texture, uv + offs);
    //             sum   += texture2D(Texture, uv - offs);
    //             count += 2.0;
    //         }
    //     }
        
    //     // average them, and output
    //     gl_FragColor = sum / count;
    // }

    // #define INV_SQRT_2PI 0.3989422804014327

    // float computeGauss(float x) {
    //     return INV_SQRT_2PI * exp(-0.5 * x * x / (Sigma * Sigma)) / Sigma;
    // }

    // vec4 blur(vec2 Direction)
    // {
    //     float factor = computeGauss(0.0);
    //     vec4 sum = texture2D(Texture, uv) * factor;

    //     float totalFactor = factor;

    //     for (int i = 1; i <= Radius; i += 1)
    //     {
    //         float x = float(i) - 0.5;
    //         factor = computeGauss(x) * 2.0;
    //         totalFactor += 2.0 * factor;
            
    //         // sum += texture2D(Texture, uv + Direction * x / Resolution) * factor;
    //         // sum += texture2D(Texture, uv - Direction * x / Resolution) * factor;
    //         sum += texture2D(Texture, uv + Direction * x / Resolution.y) * factor;
    //         sum += texture2D(Texture, uv - Direction * x / Resolution.y) * factor;
    //     }

    //     return sum / totalFactor;
    // }

    void main() {
        // vec4 color = texture2D(Texture, uv);
        // float brightness = color.r + color.g + color.b;
        // if (brightness > 0.01) {
        //     gl_FragColor = color;
        //     return;
        // }
        // vec4 blurColor = blur(Direction);
        // gl_FragColor = texture2D(Texture, uv) + blurColor;
        gl_FragColor = texture2D(Texture, uv);
        // gl_FragColor = vec4(texture2D(Texture, uv).xyz, texture2D(Texture, uv).a * 0.95);
    }
    "#; 

    let material = load_material(
        ShaderSource::Glsl {
            vertex: VERTEX_SHADER,
            fragment: FRAGMENT_SHADER,
        },
        // Default::default(),
        MaterialParams {
            uniforms: vec![
                UniformDesc::new("Resolution", UniformType::Float2),
                // UniformDesc::new("Radius", UniformType::Int1),
                // UniformDesc::new("Sigma", UniformType::Float1),
                // UniformDesc::new("Direction", UniformType::Float2),
            ],
            ..Default::default()
        },
    )
    .unwrap();

    material.set_uniform("Resolution",  vec2(screen_width(), screen_height()));
    // material.set_uniform("Radius", 20i32);
    // material.set_uniform("Sigma", 20f32);

    let rt1 = render_target(
        screen_width() as u32,
        screen_height() as u32,
    );
    let rt2 = render_target(
        screen_width() as u32,
        screen_height() as u32,
    );
    rt1.texture.set_filter(FilterMode::Nearest);
    rt2.texture.set_filter(FilterMode::Nearest);



    // ---------------- in loop

    
        // render into rt1
        set_camera(&Camera2D {
            render_target: Some(rt1.clone()),
            zoom: vec2(2.0 / screen_width(), 2.0 / screen_height()),
            target: vec2(screen_width() / 2.0, screen_height() / 2.0),
            ..Default::default()
        });
        clear_background(BLACK); // resets frame to all black
        render_frame(&mut frame_state, &mut macroquad_draw);

        // horizontal blur: rt1 -> rt2
        // set_camera(&Camera2D {
        //     render_target: Some(rt2.clone()),
        //     zoom: vec2(2.0 / screen_width(), 2.0 / screen_height()),
        //     target: vec2(screen_width() / 2.0, screen_height() / 2.0),
        //     ..Default::default()
        // });
        set_default_camera();
        gl_use_material(&material);
        // material.set_uniform("Direction", vec2(1.0, 0.0));
        draw_texture_ex(
            &rt1.texture, 0., 0., WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                ..Default::default()
            },
        );
        gl_use_default_material();
        // draw_texture_ex(
        //     &rt1.texture,
        //     0.,
        //     0.,
        //     WHITE,
        //     DrawTextureParams {
        //         dest_size: Some(vec2(screen_width(), screen_height())),
        //         ..Default::default()
        //     },
        // );

        // vertical blur: rt2 -> screen
        // set_default_camera();
        // clear_background(BLACK);
        // gl_use_material(&material);
        // material.set_uniform("Direction", vec2(0.0, 1.0));
        // draw_texture_ex(
        //     &rt2.texture, 0., 0., WHITE,
        //     DrawTextureParams {
        //         dest_size: Some(vec2(screen_width(), screen_height())),
        //         ..Default::default()
        //     },
        // );
        // gl_use_default_material();