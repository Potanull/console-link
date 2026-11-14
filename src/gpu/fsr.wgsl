// FidelityFX FSR 1, MIT, AMD. Port of pinned vendor/ffx_fsr1.h (32-bit EASU + RCAS).
struct Params { size: vec4<f32>, sharp: vec4<f32> }
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> Vertex {
 let uv=vec2<f32>(f32((index<<1u)&2u),f32(index&2u));
 return Vertex(vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),0.0,1.0),uv);
}
fn load(p:vec2<i32>)->vec3<f32> { return textureLoad(source,clamp(p,vec2<i32>(0),vec2<i32>(textureDimensions(source))-1),0).rgb; }
fn luma(c:vec3<f32>)->f32 { return c.b*0.5+(c.r*0.5+c.g); }
fn lo_rcp(a:f32)->f32 { return bitcast<f32>(0x7ef07ebbu-bitcast<u32>(a)); }
fn med_rcp(a:f32)->f32 { let b=bitcast<f32>(0x7ef19fffu-bitcast<u32>(a)); return b*(-b*a+2.0); }
fn lo_rsq(a:f32)->f32 { return bitcast<f32>(0x5f347d74u-(bitcast<u32>(a)>>1u)); }
fn easu_set(w:f32,a:f32,b:f32,c:f32,d:f32,e:f32)->vec3<f32> {
 let dx=d-b; let dy=e-a;
 let lx=clamp(abs(dx)*lo_rcp(max(abs(d-c),abs(c-b))),0.0,1.0);
 let ly=clamp(abs(dy)*lo_rcp(max(abs(e-c),abs(c-a))),0.0,1.0);
 return vec3<f32>(dx*w,dy*w,(lx*lx+ly*ly)*w);
}
fn easu_tap(off:vec2<f32>,dir:vec2<f32>,len:vec2<f32>,lob:f32,clp:f32,c:vec3<f32>)->vec4<f32> {
 let v=vec2<f32>(dot(off,dir),dot(off,vec2<f32>(-dir.y,dir.x)))*len;
 let d2=min(dot(v,v),clp);
 var wb=0.4*d2-1.0; var wa=lob*d2-1.0;
 wb*=wb; wa*=wa; wb=1.5625*wb-0.5625;
 let w=wb*wa; return vec4<f32>(c*w,w);
}
@fragment fn easu(v:Vertex)->@location(0) vec4<f32> {
 let scale=params.size.xy/params.size.zw;
 let pos=floor(v.position.xy)*scale+(0.5*scale-0.5);
 let base=vec2<i32>(floor(pos)); let pp=fract(pos);
 let b=load(base+vec2<i32>(0,-1)); let c=load(base+vec2<i32>(1,-1));
 let e=load(base+vec2<i32>(-1,0)); let f=load(base); let g=load(base+vec2<i32>(1,0)); let h=load(base+vec2<i32>(2,0));
 let i=load(base+vec2<i32>(-1,1)); let j=load(base+vec2<i32>(0,1)); let k=load(base+vec2<i32>(1,1)); let l=load(base+vec2<i32>(2,1));
 let n=load(base+vec2<i32>(0,2)); let o=load(base+vec2<i32>(1,2));
 var dl=easu_set((1.0-pp.x)*(1.0-pp.y),luma(b),luma(e),luma(f),luma(g),luma(j));
 dl+=easu_set(pp.x*(1.0-pp.y),luma(c),luma(f),luma(g),luma(h),luma(k));
 dl+=easu_set((1.0-pp.x)*pp.y,luma(f),luma(i),luma(j),luma(k),luma(n));
 dl+=easu_set(pp.x*pp.y,luma(g),luma(j),luma(k),luma(l),luma(o));
 var dir=dl.xy; let dr=dot(dir,dir); let zro=dr<1.0/32768.0;
 dir.x=select(dir.x,1.0,zro); dir*=select(lo_rsq(dr),1.0,zro);
 let len=(dl.z*0.5)*(dl.z*0.5);
 let stretch=dot(dir,dir)*lo_rcp(max(abs(dir.x),abs(dir.y)));
 let len2=vec2<f32>(1.0+(stretch-1.0)*len,1.0-0.5*len);
 let lob=0.5+((0.25-0.04)-0.5)*len; let clp=lo_rcp(lob);
 var acc=vec4<f32>(0.0);
 acc+=easu_tap(vec2<f32>(0,-1)-pp,dir,len2,lob,clp,b);
 acc+=easu_tap(vec2<f32>(1,-1)-pp,dir,len2,lob,clp,c);
 acc+=easu_tap(vec2<f32>(-1,1)-pp,dir,len2,lob,clp,i);
 acc+=easu_tap(vec2<f32>(0,1)-pp,dir,len2,lob,clp,j);
 acc+=easu_tap(vec2<f32>(0,0)-pp,dir,len2,lob,clp,f);
 acc+=easu_tap(vec2<f32>(-1,0)-pp,dir,len2,lob,clp,e);
 acc+=easu_tap(vec2<f32>(1,1)-pp,dir,len2,lob,clp,k);
 acc+=easu_tap(vec2<f32>(2,1)-pp,dir,len2,lob,clp,l);
 acc+=easu_tap(vec2<f32>(2,0)-pp,dir,len2,lob,clp,h);
 acc+=easu_tap(vec2<f32>(1,0)-pp,dir,len2,lob,clp,g);
 acc+=easu_tap(vec2<f32>(1,2)-pp,dir,len2,lob,clp,o);
 acc+=easu_tap(vec2<f32>(0,2)-pp,dir,len2,lob,clp,n);
 let rgb=clamp(acc.rgb/acc.w,min(min(f,g),min(j,k)),max(max(f,g),max(j,k)));
 return vec4<f32>(rgb,textureSampleLevel(source,linear_sampler,v.uv,0.0).a);
}
@fragment fn rcas(v:Vertex)->@location(0) vec4<f32> {
 let p=vec2<i32>(v.position.xy);
 let b=load(p+vec2<i32>(0,-1)); let d=load(p+vec2<i32>(-1,0)); let e=load(p);
 let f=load(p+vec2<i32>(1,0)); let h=load(p+vec2<i32>(0,1));
 let mn=min(min(b,d),min(f,h)); let mx=max(max(b,d),max(f,h));
 // Zero peak denominators represent a flat black/white channel: that channel must not constrain another channel.
 let hit_min=select(min(mn,e)/max(4.0*mx,vec3<f32>(1e-20)),vec3<f32>(0.0),mx==vec3<f32>(0.0));
 let hit_max=select((1.0-max(mx,e))/min(4.0*mn-4.0,vec3<f32>(-1e-20)),vec3<f32>(0.0),mn==vec3<f32>(1.0));
 let lobes=max(-hit_min,hit_max);
 let lobe=max(-0.1875,min(max(max(lobes.r,lobes.g),lobes.b),0.0))*exp2(-2.0*(1.0-params.sharp.x));
 let rgb=(lobe*b+lobe*d+lobe*h+lobe*f+e)*med_rcp(4.0*lobe+1.0);
 return vec4<f32>(clamp(rgb,vec3<f32>(0),vec3<f32>(1)),textureLoad(source,p,0).a);
}
@fragment fn standard(v:Vertex)->@location(0) vec4<f32> { return textureSampleLevel(source,linear_sampler,v.uv,0.0); }
@fragment fn present(v:Vertex)->@location(0) vec4<f32> {
 let pixel=textureSampleLevel(source,linear_sampler,v.uv,0.0);
 var rgb=pixel.rgb;
 if params.sharp.y>0.5 { rgb=select(pow((rgb+0.055)/1.055,vec3<f32>(2.4)),rgb/12.92,rgb<=vec3<f32>(0.04045)); }
 return vec4<f32>(rgb,pixel.a);
}
