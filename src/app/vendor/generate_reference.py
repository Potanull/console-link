"""Generate golden RGBA using original pinned vendor function bodies compiled as C++.
Requires g++ in PATH; no GPU/WGSL implementation is used for golden calculations.
Only shader vector/texture intrinsics and parameter qualifiers are adapted.
"""
from pathlib import Path
import re, subprocess, os, struct
p=Path(__file__).resolve().parent
nis=(p/'NIS_Scaler.h').read_text();fsr=(p/'ffx_fsr1.h').read_text();cfg=(p/'NIS_Config.h').read_text()
def function(s,name):
 a=s.index(name+'(');a=s.rfind('\n',0,a)+1;b=s.index('{',a);d=1;e=b+1
 while d:
  d+=(s[e]=='{')-(s[e]=='}');e+=1
 return s[a:e]
coefficients=[]
for name in ['coef_scale','coef_usm']:
 table=cfg.split('constexpr float '+name+'[kPhaseCount][kFilterSize] = {')[1].split('};')[0]
 for row in re.findall(r'\{([^}]+)\}',table):
  coefficients.extend(float(x.strip().removesuffix('f')) for x in row.split(',') if x.strip())
(p/'nis-coefficients.bin').write_bytes(struct.pack('<'+str(len(coefficients))+'f',*coefficients))
base=r'''
#include <cmath>
#include <algorithm>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <vector>
using std::min;using std::max;using std::abs;
template<int N> struct V {union {struct{float x,y,z,w;};struct{float r,g,b,a;};float v[4];};
 V(){for(auto &a:v)a=0;} V(float a){for(auto &b:v)b=a;} V(float a,float b){x=a;y=b;z=w=0;}
 V(float a,float b,float c){x=a;y=b;z=c;w=0;} V(float a,float b,float c,float d){x=a;y=b;z=c;w=d;}
 V<2> xy()const{return V<2>(x,y);} V<2> zw()const{return V<2>(z,w);} V<3> rgb()const{return V<3>(x,y,z);}
 V& operator+=(V b){for(int i=0;i<N;i++)v[i]+=b.v[i];return *this;}
 V& operator-=(V b){for(int i=0;i<N;i++)v[i]-=b.v[i];return *this;}
 V& operator*=(V b){for(int i=0;i<N;i++)v[i]*=b.v[i];return *this;}
};
#define OP(o) template<int N> V<N> operator o(V<N>a,V<N>b){V<N>r;for(int i=0;i<N;i++)r.v[i]=a.v[i] o b.v[i];return r;} template<int N> V<N> operator o(V<N>a,float b){return a o V<N>(b);} template<int N> V<N> operator o(float a,V<N>b){return V<N>(a) o b;}
OP(+) OP(-) OP(*) OP(/)
template<int N> V<N> min(V<N>a,V<N>b){V<N>r;for(int i=0;i<N;i++)r.v[i]=min(a.v[i],b.v[i]);return r;}
template<int N> V<N> max(V<N>a,V<N>b){V<N>r;for(int i=0;i<N;i++)r.v[i]=max(a.v[i],b.v[i]);return r;}
template<int N> V<N> floor(V<N>a){for(int i=0;i<N;i++)a.v[i]=std::floor(a.v[i]);return a;}
using NVF=float;using NVI=int;using NVB=bool;using NVF4=V<4>;
using AF1=float;using AF2=V<2>;using AF3=V<3>;using AF4=V<4>;using AU2=V<2>;using AU4=V<4>;using ASU2=V<2>;using AP1=bool;
#define AF1_(x) float(x)
#define AF2_(x) AF2(x)
#define AF3_(x) AF3(x)
#define AF4_(x) AF4(x)
#define AF2_AU2(x) (x)
#define AF1_AU1(x) (x)
#define NIS_UNROLL
#define NIS_SCALE_FLOAT 1.0f
#define kPhaseCount 64
#define FSR_RCAS_LIMIT 0.1875f
float saturate(float a){return std::clamp(a,0.f,1.f);} float ASatF1(float a){return saturate(a);}
template<class T>T lerp(T a,T b,float t){return a+(b-a)*t;}
float ARcpF1(float a){return 1.f/a;}float APrxLoRcpF1(float a){uint32_t u;memcpy(&u,&a,4);u=0x7ef07ebb-u;memcpy(&a,&u,4);return a;}
float APrxLoRsqF1(float a){uint32_t u;memcpy(&u,&a,4);u=0x5f347d74-(u>>1);memcpy(&a,&u,4);return a;}
float APrxMedRcpF1(float a){uint32_t u;float b;memcpy(&u,&a,4);u=0x7ef19fff-u;memcpy(&b,&u,4);return b*(-b*a+2.f);}
template<class T>T AMin3F3(T a,T b,T c){return min(min(a,b),c);} template<class T>T AMax3F3(T a,T b,T c){return max(max(a,b),c);}
float AMin3F1(float a,float b,float c){return min(min(a,b),c);}float AMax3F1(float a,float b,float c){return max(max(a,b),c);}
float kDetectRatio=2.f*1127.f/1024.f,kDetectThres=64.f/1024.f,kEps=1.f/255.f,kMinContrastRatio=2.f,kRatioNorm=0.125f,kContrastBoost=1.f;
float kSharpStartY=.45f,kSharpScaleY=1.f/.45f,kSharpStrengthMin,kSharpStrengthScale,kSharpLimitMin,kSharpLimitScale;
int width=17,height=13;std::vector<AF4> source;
AF4 load(int x,int y){return source[std::clamp(y,0,height-1)*width+std::clamp(x,0,width-1)];}
AF4 bilinear(float x,float y){int ix=floor(x),iy=floor(y);float fx=x-ix,fy=y-iy;return lerp(lerp(load(ix,iy),load(ix+1,iy),fx),lerp(load(ix,iy+1),load(ix+1,iy+1),fx),fy);}
AF4 gather(AF2 p,int c){int x=floor(p.x*width-.5f),y=floor(p.y*height-.5f);return AF4(load(x,y+1).v[c],load(x+1,y+1).v[c],load(x+1,y).v[c],load(x,y).v[c]);}
AF4 FsrEasuRF(AF2 p){return gather(p,0);}AF4 FsrEasuGF(AF2 p){return gather(p,1);}AF4 FsrEasuBF(AF2 p){return gather(p,2);}
AF4 FsrRcasLoadF(ASU2 p){return load(int(p.x),int(p.y));}void FsrRcasInputF(float&,float&,float&){}
'''
for name,var in [('coef_scale','shCoefScaler'),('coef_usm','shCoefUSM')]:
 t=cfg.split('constexpr float '+name+'[kPhaseCount][kFilterSize] = {')[1].split('};')[0]
 base+='float '+var+'[64][8]={'+t+'};\n'
for name in ['GetEdgeMap','CalcLTI','EvalPoly6','FilterNormal','AddDirFilters']:
 f=function(nis,name)
 if name=='GetEdgeMap': f='NVF4 GetEdgeMap(NVF p[6][6], NVI i, NVI j)'+f[f.index('{'):]
 base+=f+'\n'
for name in ['FsrEasuTapF','FsrEasuSetF','FsrEasuF','FsrRcasF']:
 f=function(fsr,name)
 f=re.sub(r'\b(?:inout|out)\s+(\w+)\s+(\w+)',r'\1 &\2',f)
 for a in ['xy','zw','rgb']: f=f.replace('.'+a,'.'+a+'()')
 base+=f+'\n'
base+=r'''
void save(const char*name,std::vector<AF4>&pixels){std::ofstream f(name,std::ios::binary);for(auto p:pixels)for(int i=0;i<4;i++){unsigned char b=std::clamp(int(std::nearbyint(p.v[i]*255.f)),0,255);f.write((char*)&b,1);}}
int main(){
 for(int y=0;y<height;y++)for(int x=0;x<width;x++){
  int r,g,b;
  if(y<3){r=g=b=16+x*224/(width-1);}else if(y<7){r=(x>y+2)?230:24;g=(x+y>16)?215:40;b=(x<width-y)?195:60;}
  else if(y<10){r=g=b=((x+y)%2)?220:32;}else{r=20+(x*37+y*11)%215;g=18+(x*13+y*43)%217;b=22+(x*71+y*19)%210;}
  source.emplace_back(r/255.f,g/255.f,b/255.f,1.f);
 }
 save("reference-input.rgba",source);auto original=source;
 int tw=29,th=21;
 for(int sharp_idx=0;sharp_idx<3;sharp_idx++){
 float sharp=sharp_idx==0?0.f:sharp_idx==1?.3f:1.f;
 float slider=sharp-.5f;float maxs=slider>=0?1.25f:1.75f,mins=slider>=0?1.25f:1.f;
 kSharpStrengthMin=max(0.f,.4f+slider*mins*1.2f);kSharpStrengthScale=1.6f+slider*maxs*1.8f-kSharpStrengthMin;
 kSharpLimitMin=max(.1f,.14f+slider*mins*.32f);kSharpLimitScale=.5f+slider*mins*.6f-kSharpLimitMin;
 std::vector<AF4> out;
 for(int y=0;y<th;y++)for(int x=0;x<tw;x++){
 float sx=(x+.5f)*width/tw-.5f,sy=(y+.5f)*height/th-.5f;int ix=floor(sx),iy=floor(sy);float fx=sx-ix,fy=sy-iy;float p[6][6];
 for(int i=0;i<6;i++)for(int j=0;j<6;j++){auto c=load(ix+j-2,iy+i-2);p[i][j]=c.r*.2126f+c.g*.7152f+c.b*.0722f;}
 auto w=lerp(lerp(GetEdgeMap(p,1,1),GetEdgeMap(p,1,2),fx),lerp(GetEdgeMap(p,2,1),GetEdgeMap(p,2,2),fx),fy);
 float oy=FilterNormal(p,int(fx*64),int(fy*64))*(1-w.x-w.y-w.z-w.w)+AddDirFilters(p,fx,fy,int(fx*64),int(fy*64),w);
 auto c=bilinear(sx,sy);float corr=oy-(c.r*.2126f+c.g*.7152f+c.b*.0722f);c.r=saturate(c.r+corr);c.g=saturate(c.g+corr);c.b=saturate(c.b+corr);out.push_back(c);
 }
 save((std::string("nis-")+std::to_string(sharp_idx)+".rgba").c_str(),out);out.clear();
 AU4 c0(float(width)/tw,float(height)/th,.5f*width/tw-.5f,.5f*height/th-.5f),c1(1.f/width,1.f/height,1.f/width,-1.f/height),c2(-1.f/width,2.f/height,1.f/width,2.f/height),c3(0.f,4.f/height,0.f,0.f);
 for(int y=0;y<th;y++)for(int x=0;x<tw;x++){AF3 pix;FsrEasuF(pix,AU2(x,y),c0,c1,c2,c3);AF4 o(pix.r,pix.g,pix.b,1.f);for(int c=0;c<3;c++)o.v[c]=std::nearbyint(o.v[c]*255.f)/255.f;out.push_back(o);}
 save("fsr-easu.rgba",out);source=out;width=tw;height=th;out.clear();
 for(int y=0;y<th;y++)for(int x=0;x<tw;x++){float r,g,b;FsrRcasF(r,g,b,AU2(x,y),AU4(std::exp2(-2.f*(1.f-sharp)),0,0,0));out.emplace_back(r,g,b,1.f);}
 save((std::string("fsr-")+std::to_string(sharp_idx)+".rgba").c_str(),out);source=original;width=17;height=13;
 }
}
'''
(p/'reference.cpp').write_text(base)
compiler=os.environ.get('CXX','g++')
subprocess.run([compiler,'-std=c++17','-O2','-ffp-contract=off','-static','reference.cpp','-o','reference.exe'],cwd=p,check=True)
subprocess.run([str(p/'reference.exe')],cwd=p,check=True)
(p/'reference.exe').unlink()
print('Generated pinned upstream C++ golden outputs: NIS/FSR, sharpness 0/.3/1, 17x13 -> 29x21')
