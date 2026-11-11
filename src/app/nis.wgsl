// NVIDIA Image Scaling v1.0.3, MIT (vendor/NIS_LICENSE.txt). FP32 SDR equation port.
struct Params { size: vec4<f32>, sharp: vec4<f32> }
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> Vertex {
 let uv=vec2<f32>(f32((index<<1u)&2u),f32(index&2u));
 return Vertex(vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),0.0,1.0),uv);
}
fn sample(p:vec2<i32>)->vec4<f32> { return textureLoad(source,clamp(p,vec2<i32>(0),vec2<i32>(textureDimensions(source))-1),0); }
fn getY(c:vec3<f32>)->f32 { return dot(c,vec3<f32>(0.2126,0.7152,0.0722)); }
fn sat(x:f32)->f32 { return clamp(x,0.0,1.0); }
struct Config { strength_min:f32, strength_scale:f32, limit_min:f32, limit_scale:f32 }
fn config()->Config {
 let slider=params.sharp.x-0.5;
 let max_scale=select(1.75,1.25,slider>=0.0);
 let min_scale=select(1.0,1.25,slider>=0.0);
 let limit_scale=min_scale;
 let smin=max(0.0,0.4+slider*min_scale*1.2);
 let smax=1.6+slider*max_scale*1.8;
 let lmin=max(0.1,0.14+slider*limit_scale*0.32);
 let lmax=0.5+slider*limit_scale*0.6;
 return Config(smin,smax-smin,lmin,lmax-lmin);
}
struct CoefficientBank { values:array<vec4<f32>,256> }
@group(0) @binding(3) var<uniform> bank:CoefficientBank;
fn coefficient(phase:i32,tap:i32,usm:i32)->f32 { return bank.values[usm*128+phase*2+tap/4][tap%4]; }
fn GetEdgeMap(p:array<array<f32,6>,6>,i:i32,j:i32)->vec4<f32>{
    var g_0:f32 = abs(p[0 + i][0 + j] + p[0 + i][1 + j] + p[0 + i][2 + j] - p[2 + i][0 + j] - p[2 + i][1 + j] - p[2 + i][2 + j]);
    var g_45:f32 = abs(p[1 + i][0 + j] + p[0 + i][0 + j] + p[0 + i][1 + j] - p[2 + i][1 + j] - p[2 + i][2 + j] - p[1 + i][2 + j]);
    var g_90:f32 = abs(p[0 + i][0 + j] + p[1 + i][0 + j] + p[2 + i][0 + j] - p[0 + i][2 + j] - p[1 + i][2 + j] - p[2 + i][2 + j]);
    var g_135:f32 = abs(p[1 + i][0 + j] + p[2 + i][0 + j] + p[2 + i][1 + j] - p[0 + i][1 + j] - p[0 + i][2 + j] - p[1 + i][2 + j]);

    var g_0_90_max:f32 = max(g_0, g_90);
    var g_0_90_min:f32 = min(g_0, g_90);
    var g_45_135_max:f32 = max(g_45, g_135);
    var g_45_135_min:f32 = min(g_45, g_135);

    var e_0_90:f32 = 0;
    var e_45_135:f32 = 0;

    if (g_0_90_max + g_45_135_max == 0)
    {
        return vec4<f32>(0, 0, 0, 0);
    }

    e_0_90 = min(g_0_90_max / (g_0_90_max + g_45_135_max), 1.0);
    e_45_135 = 1.0 - e_0_90;

    var c_0_90:bool = (g_0_90_max > (g_0_90_min * (2.0*1127.0/1024.0))) && (g_0_90_max > (64.0/1024.0)) && (g_0_90_max > g_45_135_min);
    var c_45_135:bool = (g_45_135_max > (g_45_135_min * (2.0*1127.0/1024.0))) && (g_45_135_max > (64.0/1024.0)) && (g_45_135_max > g_0_90_min);
    var c_g_0_90:bool = g_0_90_max == g_0;
    var c_g_45_135:bool = g_45_135_max == g_45;

    var f_e_0_90:f32 = select(1.0, e_0_90, (c_0_90 && c_45_135));
    var f_e_45_135:f32 = select(1.0, e_45_135, (c_0_90 && c_45_135));

    var weight_0:f32 = select(0.0, f_e_0_90, (c_0_90 && c_g_0_90));
    var weight_90:f32 = select(0.0, f_e_0_90, (c_0_90 && !c_g_0_90));
    var weight_45:f32 = select(0.0, f_e_45_135, (c_45_135 && c_g_45_135));
    var weight_135:f32 = select(0.0, f_e_45_135, (c_45_135 && !c_g_45_135));

    return vec4<f32>(weight_0, weight_90, weight_45, weight_135);
}
fn CalcLTI(p0:f32,p1:f32,p2:f32,p3:f32,p4:f32,p5:f32,phase_index:i32)->f32{
    var selector:bool = (phase_index <= 64 / 2);
    var sel:f32 = select(p3, p0, selector);
    var a_min:f32 = min(min(p1, p2), sel);
    var a_max:f32 = max(max(p1, p2), sel);
    sel = select(p5, p2, selector);
    var b_min:f32 = min(min(p3, p4), sel);
    var b_max:f32 = max(max(p3, p4), sel);

    var a_cont:f32 = a_max - a_min;
    var b_cont:f32 = b_max - b_min;

    var cont_ratio:f32 = max(a_cont, b_cont) / (min(a_cont, b_cont) + (1.0/255.0));
    return (1.0 - sat((cont_ratio - 2.0) * 0.125)) * 1.0;
}
fn EvalPoly6(pxl:array<f32,6>,phase_int:i32)->f32{
    var y:f32 = 0.0;
    {

        for (var i:i32 = 0; i < 6; i++)
        {
            y += coefficient(phase_int,i,0) * pxl[i];
        }
    }
    var y_usm:f32 = 0.0;
    {

        for (var i:i32 = 0; i < 6; i++)
        {
            y_usm += coefficient(phase_int,i,1) * pxl[i];
        }
    }


    var y_scale:f32 = 1.0 - sat((y * (1.0 / 1.0) - 0.45) * (1.0/0.45));


    var y_sharpness:f32 = y_scale * config().strength_scale + config().strength_min;

    y_usm *= y_sharpness;


    var y_sharpness_limit:f32 = (y_scale * config().limit_scale + config().limit_min) * y;

    y_usm = min(y_sharpness_limit, max(-y_sharpness_limit, y_usm));

    y_usm *= CalcLTI(pxl[0], pxl[1], pxl[2], pxl[3], pxl[4], pxl[5], phase_int);

    return y + y_usm;
}
fn FilterNormal(p:array<array<f32,6>,6>,phase_x_frac_int:i32,phase_y_frac_int:i32)->f32{
    var h_acc:f32 = 0.0;

    for (var j:i32 = 0; j < 6; j++)
    {
        var v_acc:f32 = 0.0;

        for (var i:i32 = 0; i < 6; i++)
        {
            v_acc += p[i][j] * coefficient(phase_y_frac_int,i,0);
        }
        h_acc += v_acc * coefficient(phase_x_frac_int,j,0);
    }


    return h_acc;
}
fn AddDirFilters(p:array<array<f32,6>,6>,phase_x_frac:f32,phase_y_frac:f32,phase_x_frac_int:i32,phase_y_frac_int:i32,w:vec4<f32>)->f32{
    var f:f32 = 0;
    if (w.x > 0.0)
    {

        var interp0Deg:array<f32,6>;
        {

                for (var i:i32 = 0; i < 6; i++)
                {
                    interp0Deg[i] = mix(p[i][2], p[i][3], phase_x_frac);
                }
        }
        f += EvalPoly6(interp0Deg, phase_y_frac_int) * w.x;
    }
    if (w.y > 0.0)
    {

        var interp90Deg:array<f32,6>;
        {

                for (var i:i32 = 0; i < 6; i++)
                {
                    interp90Deg[i] = mix(p[2][i], p[3][i], phase_y_frac);
                }
        }

        f += EvalPoly6(interp90Deg, phase_x_frac_int) * w.y;
    }
    if (w.z > 0.0)
    {

        var pphase_b45:f32 = 0.5 + 0.5 * (phase_x_frac - phase_y_frac);

        var temp_interp45Deg:array<f32,7>;
        temp_interp45Deg[1] = mix(p[2][1], p[1][2], pphase_b45);
        temp_interp45Deg[3] = mix(p[3][2], p[2][3], pphase_b45);
        temp_interp45Deg[5] = mix(p[4][3], p[3][4], pphase_b45);
        {
            pphase_b45 = pphase_b45 - 0.5;
            var a:f32 = select(p[2][0], p[0][2], (pphase_b45 >= 0.0));
            var b:f32 = select(p[3][1], p[1][3], (pphase_b45 >= 0.0));
            var c:f32 = select(p[4][2], p[2][4], (pphase_b45 >= 0.0));
            var d:f32 = select(p[5][3], p[3][5], (pphase_b45 >= 0.0));
            temp_interp45Deg[0] = mix(p[1][1], a, abs(pphase_b45));
            temp_interp45Deg[2] = mix(p[2][2], b, abs(pphase_b45));
            temp_interp45Deg[4] = mix(p[3][3], c, abs(pphase_b45));
            temp_interp45Deg[6] = mix(p[4][4], d, abs(pphase_b45));
        }

        var interp45Deg:array<f32,6>;
        var pphase_p45:f32 = phase_x_frac + phase_y_frac;
        if (pphase_p45 >= 1)
        {

                for (var i:i32 = 0; i < 6; i++)
                {
                    interp45Deg[i] = temp_interp45Deg[i + 1];
                }
            pphase_p45 = pphase_p45 - 1;
        }
        else
        {

                for (var i:i32 = 0; i < 6; i++)
                {
                    interp45Deg[i] = temp_interp45Deg[i];
                }
        }

        f += EvalPoly6(interp45Deg, i32(pphase_p45 * 64)) * w.z;
    }
    if (w.w > 0.0)
    {

        var pphase_b135:f32 = 0.5 * (phase_x_frac + phase_y_frac);

        var temp_interp135Deg:array<f32,7>;
        temp_interp135Deg[1] = mix(p[3][1], p[4][2], pphase_b135);
        temp_interp135Deg[3] = mix(p[2][2], p[3][3], pphase_b135);
        temp_interp135Deg[5] = mix(p[1][3], p[2][4], pphase_b135);
        {
            pphase_b135 = pphase_b135 - 0.5;
            var a:f32 = select(p[3][0], p[5][2], (pphase_b135 >= 0.0));
            var b:f32 = select(p[2][1], p[4][3], (pphase_b135 >= 0.0));
            var c:f32 = select(p[1][2], p[3][4], (pphase_b135 >= 0.0));
            var d:f32 = select(p[0][3], p[2][5], (pphase_b135 >= 0.0));
            temp_interp135Deg[0] = mix(p[4][1], a, abs(pphase_b135));
            temp_interp135Deg[2] = mix(p[3][2], b, abs(pphase_b135));
            temp_interp135Deg[4] = mix(p[2][3], c, abs(pphase_b135));
            temp_interp135Deg[6] = mix(p[1][4], d, abs(pphase_b135));
        }

        var interp135Deg:array<f32,6>;
        var pphase_p135:f32 = 1 + (phase_x_frac - phase_y_frac);
        if (pphase_p135 >= 1)
        {

                for (var i:i32 = 0; i < 6; i++)
                {
                    interp135Deg[i] = temp_interp135Deg[i + 1];
                }
            pphase_p135 = pphase_p135 - 1;
        }
        else
        {

                for (var i:i32 = 0; i < 6; i++)
                {
                    interp135Deg[i] = temp_interp135Deg[i];
                }
        }

        f += EvalPoly6(interp135Deg, i32(pphase_p135 * 64)) * w.w;
    }
    return f;
}

@fragment fn nis(v:Vertex)->@location(0) vec4<f32> {
 let pos=v.position.xy*params.size.xy/params.size.zw-0.5;
 let base=vec2<i32>(floor(pos)); let phase=fract(pos);
 var p:array<array<f32,6>,6>;
 for(var y=0;y<6;y++) { for(var x=0;x<6;x++) { p[y][x]=getY(sample(base+vec2<i32>(x-2,y-2)).rgb); } }
 let e00=GetEdgeMap(p,1,1); let e01=GetEdgeMap(p,1,2);
 let e10=GetEdgeMap(p,2,1); let e11=GetEdgeMap(p,2,2);
 let weights=mix(mix(e00,e01,phase.x),mix(e10,e11,phase.x),phase.y);
 let ix=i32(phase.x*64.0); let iy=i32(phase.y*64.0);
 let y=FilterNormal(p,ix,iy)*(1.0-weights.x-weights.y-weights.z-weights.w)+AddDirFilters(p,phase.x,phase.y,ix,iy,weights);
 let pixel=textureSampleLevel(source,linear_sampler,v.uv,0.0);
 return vec4<f32>(clamp(pixel.rgb+vec3<f32>(y-getY(pixel.rgb)),vec3<f32>(0.0),vec3<f32>(1.0)),pixel.a);
}
