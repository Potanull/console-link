
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
float shCoefScaler[64][8]={
        {0.0f,     0.0f,    1.0000f, 0.0f,     0.0f,    0.0f, 0.0f, 0.0f},
        {0.0029f, -0.0127f, 1.0000f, 0.0132f, -0.0034f, 0.0f, 0.0f, 0.0f},
        {0.0063f, -0.0249f, 0.9985f, 0.0269f, -0.0068f, 0.0f, 0.0f, 0.0f},
        {0.0088f, -0.0361f, 0.9956f, 0.0415f, -0.0103f, 0.0005f, 0.0f, 0.0f},
        {0.0117f, -0.0474f, 0.9932f, 0.0562f, -0.0142f, 0.0005f, 0.0f, 0.0f},
        {0.0142f, -0.0576f, 0.9897f, 0.0713f, -0.0181f, 0.0005f, 0.0f, 0.0f},
        {0.0166f, -0.0674f, 0.9844f, 0.0874f, -0.0220f, 0.0010f, 0.0f, 0.0f},
        {0.0186f, -0.0762f, 0.9785f, 0.1040f, -0.0264f, 0.0015f, 0.0f, 0.0f},
        {0.0205f, -0.0850f, 0.9727f, 0.1206f, -0.0308f, 0.0020f, 0.0f, 0.0f},
        {0.0225f, -0.0928f, 0.9648f, 0.1382f, -0.0352f, 0.0024f, 0.0f, 0.0f},
        {0.0239f, -0.1006f, 0.9575f, 0.1558f, -0.0396f, 0.0029f, 0.0f, 0.0f},
        {0.0254f, -0.1074f, 0.9487f, 0.1738f, -0.0439f, 0.0034f, 0.0f, 0.0f},
        {0.0264f, -0.1138f, 0.9390f, 0.1929f, -0.0488f, 0.0044f, 0.0f, 0.0f},
        {0.0278f, -0.1191f, 0.9282f, 0.2119f, -0.0537f, 0.0049f, 0.0f, 0.0f},
        {0.0288f, -0.1245f, 0.9170f, 0.2310f, -0.0581f, 0.0059f, 0.0f, 0.0f},
        {0.0293f, -0.1294f, 0.9058f, 0.2510f, -0.0630f, 0.0063f, 0.0f, 0.0f},
        {0.0303f, -0.1333f, 0.8926f, 0.2710f, -0.0679f, 0.0073f, 0.0f, 0.0f},
        {0.0308f, -0.1367f, 0.8789f, 0.2915f, -0.0728f, 0.0083f, 0.0f, 0.0f},
        {0.0308f, -0.1401f, 0.8657f, 0.3120f, -0.0776f, 0.0093f, 0.0f, 0.0f},
        {0.0313f, -0.1426f, 0.8506f, 0.3330f, -0.0825f, 0.0103f, 0.0f, 0.0f},
        {0.0313f, -0.1445f, 0.8354f, 0.3540f, -0.0874f, 0.0112f, 0.0f, 0.0f},
        {0.0313f, -0.1460f, 0.8193f, 0.3755f, -0.0923f, 0.0122f, 0.0f, 0.0f},
        {0.0313f, -0.1470f, 0.8022f, 0.3965f, -0.0967f, 0.0137f, 0.0f, 0.0f},
        {0.0308f, -0.1479f, 0.7856f, 0.4185f, -0.1016f, 0.0146f, 0.0f, 0.0f},
        {0.0303f, -0.1479f, 0.7681f, 0.4399f, -0.1060f, 0.0156f, 0.0f, 0.0f},
        {0.0298f, -0.1479f, 0.7505f, 0.4614f, -0.1104f, 0.0166f, 0.0f, 0.0f},
        {0.0293f, -0.1470f, 0.7314f, 0.4829f, -0.1147f, 0.0181f, 0.0f, 0.0f},
        {0.0288f, -0.1460f, 0.7119f, 0.5049f, -0.1187f, 0.0190f, 0.0f, 0.0f},
        {0.0278f, -0.1445f, 0.6929f, 0.5264f, -0.1226f, 0.0200f, 0.0f, 0.0f},
        {0.0273f, -0.1431f, 0.6724f, 0.5479f, -0.1260f, 0.0215f, 0.0f, 0.0f},
        {0.0264f, -0.1411f, 0.6528f, 0.5693f, -0.1299f, 0.0225f, 0.0f, 0.0f},
        {0.0254f, -0.1387f, 0.6323f, 0.5903f, -0.1328f, 0.0234f, 0.0f, 0.0f},
        {0.0244f, -0.1357f, 0.6113f, 0.6113f, -0.1357f, 0.0244f, 0.0f, 0.0f},
        {0.0234f, -0.1328f, 0.5903f, 0.6323f, -0.1387f, 0.0254f, 0.0f, 0.0f},
        {0.0225f, -0.1299f, 0.5693f, 0.6528f, -0.1411f, 0.0264f, 0.0f, 0.0f},
        {0.0215f, -0.1260f, 0.5479f, 0.6724f, -0.1431f, 0.0273f, 0.0f, 0.0f},
        {0.0200f, -0.1226f, 0.5264f, 0.6929f, -0.1445f, 0.0278f, 0.0f, 0.0f},
        {0.0190f, -0.1187f, 0.5049f, 0.7119f, -0.1460f, 0.0288f, 0.0f, 0.0f},
        {0.0181f, -0.1147f, 0.4829f, 0.7314f, -0.1470f, 0.0293f, 0.0f, 0.0f},
        {0.0166f, -0.1104f, 0.4614f, 0.7505f, -0.1479f, 0.0298f, 0.0f, 0.0f},
        {0.0156f, -0.1060f, 0.4399f, 0.7681f, -0.1479f, 0.0303f, 0.0f, 0.0f},
        {0.0146f, -0.1016f, 0.4185f, 0.7856f, -0.1479f, 0.0308f, 0.0f, 0.0f},
        {0.0137f, -0.0967f, 0.3965f, 0.8022f, -0.1470f, 0.0313f, 0.0f, 0.0f},
        {0.0122f, -0.0923f, 0.3755f, 0.8193f, -0.1460f, 0.0313f, 0.0f, 0.0f},
        {0.0112f, -0.0874f, 0.3540f, 0.8354f, -0.1445f, 0.0313f, 0.0f, 0.0f},
        {0.0103f, -0.0825f, 0.3330f, 0.8506f, -0.1426f, 0.0313f, 0.0f, 0.0f},
        {0.0093f, -0.0776f, 0.3120f, 0.8657f, -0.1401f, 0.0308f, 0.0f, 0.0f},
        {0.0083f, -0.0728f, 0.2915f, 0.8789f, -0.1367f, 0.0308f, 0.0f, 0.0f},
        {0.0073f, -0.0679f, 0.2710f, 0.8926f, -0.1333f, 0.0303f, 0.0f, 0.0f},
        {0.0063f, -0.0630f, 0.2510f, 0.9058f, -0.1294f, 0.0293f, 0.0f, 0.0f},
        {0.0059f, -0.0581f, 0.2310f, 0.9170f, -0.1245f, 0.0288f, 0.0f, 0.0f},
        {0.0049f, -0.0537f, 0.2119f, 0.9282f, -0.1191f, 0.0278f, 0.0f, 0.0f},
        {0.0044f, -0.0488f, 0.1929f, 0.9390f, -0.1138f, 0.0264f, 0.0f, 0.0f},
        {0.0034f, -0.0439f, 0.1738f, 0.9487f, -0.1074f, 0.0254f, 0.0f, 0.0f},
        {0.0029f, -0.0396f, 0.1558f, 0.9575f, -0.1006f, 0.0239f, 0.0f, 0.0f},
        {0.0024f, -0.0352f, 0.1382f, 0.9648f, -0.0928f, 0.0225f, 0.0f, 0.0f},
        {0.0020f, -0.0308f, 0.1206f, 0.9727f, -0.0850f, 0.0205f, 0.0f, 0.0f},
        {0.0015f, -0.0264f, 0.1040f, 0.9785f, -0.0762f, 0.0186f, 0.0f, 0.0f},
        {0.0010f, -0.0220f, 0.0874f, 0.9844f, -0.0674f, 0.0166f, 0.0f, 0.0f},
        {0.0005f, -0.0181f, 0.0713f, 0.9897f, -0.0576f, 0.0142f, 0.0f, 0.0f},
        {0.0005f, -0.0142f, 0.0562f, 0.9932f, -0.0474f, 0.0117f, 0.0f, 0.0f},
        {0.0005f, -0.0103f, 0.0415f, 0.9956f, -0.0361f, 0.0088f, 0.0f, 0.0f},
        {0.0f, -0.0068f, 0.0269f, 0.9985f, -0.0249f, 0.0063f, 0.0f, 0.0f},
        {0.0f, -0.0034f, 0.0132f, 1.0000f, -0.0127f, 0.0029f, 0.0f, 0.0f}
    };
float shCoefUSM[64][8]={
        {0.0f,      -0.6001f, 1.2002f, -0.6001f,  0.0f,  0.0f, 0.0f, 0.0f},
        {0.0029f, -0.6084f, 1.1987f, -0.5903f, -0.0029f, 0.0f, 0.0f, 0.0f},
        {0.0049f, -0.6147f, 1.1958f, -0.5791f, -0.0068f, 0.0005f, 0.0f, 0.0f},
        {0.0073f, -0.6196f, 1.1890f, -0.5659f, -0.0103f, 0.0f, 0.0f, 0.0f},
        {0.0093f, -0.6235f, 1.1802f, -0.5513f, -0.0151f, 0.0f, 0.0f, 0.0f},
        {0.0112f, -0.6265f, 1.1699f, -0.5352f, -0.0195f, 0.0005f, 0.0f, 0.0f},
        {0.0122f, -0.6270f, 1.1582f, -0.5181f, -0.0259f, 0.0005f, 0.0f, 0.0f},
        {0.0142f, -0.6284f, 1.1455f, -0.5005f, -0.0317f, 0.0005f, 0.0f, 0.0f},
        {0.0156f, -0.6265f, 1.1274f, -0.4790f, -0.0386f, 0.0005f, 0.0f, 0.0f},
        {0.0166f, -0.6235f, 1.1089f, -0.4570f, -0.0454f, 0.0010f, 0.0f, 0.0f},
        {0.0176f, -0.6187f, 1.0879f, -0.4346f, -0.0532f, 0.0010f, 0.0f, 0.0f},
        {0.0181f, -0.6138f, 1.0659f, -0.4102f, -0.0615f, 0.0015f, 0.0f, 0.0f},
        {0.0190f, -0.6069f, 1.0405f, -0.3843f, -0.0698f, 0.0015f, 0.0f, 0.0f},
        {0.0195f, -0.6006f, 1.0161f, -0.3574f, -0.0796f, 0.0020f, 0.0f, 0.0f},
        {0.0200f, -0.5928f, 0.9893f, -0.3286f, -0.0898f, 0.0024f, 0.0f, 0.0f},
        {0.0200f, -0.5820f, 0.9580f, -0.2988f, -0.1001f, 0.0029f, 0.0f, 0.0f},
        {0.0200f, -0.5728f, 0.9292f, -0.2690f, -0.1104f, 0.0034f, 0.0f, 0.0f},
        {0.0200f, -0.5620f, 0.8975f, -0.2368f, -0.1226f, 0.0039f, 0.0f, 0.0f},
        {0.0205f, -0.5498f, 0.8643f, -0.2046f, -0.1343f, 0.0044f, 0.0f, 0.0f},
        {0.0200f, -0.5371f, 0.8301f, -0.1709f, -0.1465f, 0.0049f, 0.0f, 0.0f},
        {0.0195f, -0.5239f, 0.7944f, -0.1367f, -0.1587f, 0.0054f, 0.0f, 0.0f},
        {0.0195f, -0.5107f, 0.7598f, -0.1021f, -0.1724f, 0.0059f, 0.0f, 0.0f},
        {0.0190f, -0.4966f, 0.7231f, -0.0649f, -0.1865f, 0.0063f, 0.0f, 0.0f},
        {0.0186f, -0.4819f, 0.6846f, -0.0288f, -0.1997f, 0.0068f, 0.0f, 0.0f},
        {0.0186f, -0.4668f, 0.6460f, 0.0093f, -0.2144f, 0.0073f, 0.0f, 0.0f},
        {0.0176f, -0.4507f, 0.6055f, 0.0479f, -0.2290f, 0.0083f, 0.0f, 0.0f},
        {0.0171f, -0.4370f, 0.5693f, 0.0859f, -0.2446f, 0.0088f, 0.0f, 0.0f},
        {0.0161f, -0.4199f, 0.5283f, 0.1255f, -0.2598f, 0.0098f, 0.0f, 0.0f},
        {0.0161f, -0.4048f, 0.4883f, 0.1655f, -0.2754f, 0.0103f, 0.0f, 0.0f},
        {0.0151f, -0.3887f, 0.4497f, 0.2041f, -0.2910f, 0.0107f, 0.0f, 0.0f},
        {0.0142f, -0.3711f, 0.4072f, 0.2446f, -0.3066f, 0.0117f, 0.0f, 0.0f},
        {0.0137f, -0.3555f, 0.3672f, 0.2852f, -0.3228f, 0.0122f, 0.0f, 0.0f},
        {0.0132f, -0.3394f, 0.3262f, 0.3262f, -0.3394f, 0.0132f, 0.0f, 0.0f},
        {0.0122f, -0.3228f, 0.2852f, 0.3672f, -0.3555f, 0.0137f, 0.0f, 0.0f},
        {0.0117f, -0.3066f, 0.2446f, 0.4072f, -0.3711f, 0.0142f, 0.0f, 0.0f},
        {0.0107f, -0.2910f, 0.2041f, 0.4497f, -0.3887f, 0.0151f, 0.0f, 0.0f},
        {0.0103f, -0.2754f, 0.1655f, 0.4883f, -0.4048f, 0.0161f, 0.0f, 0.0f},
        {0.0098f, -0.2598f, 0.1255f, 0.5283f, -0.4199f, 0.0161f, 0.0f, 0.0f},
        {0.0088f, -0.2446f, 0.0859f, 0.5693f, -0.4370f, 0.0171f, 0.0f, 0.0f},
        {0.0083f, -0.2290f, 0.0479f, 0.6055f, -0.4507f, 0.0176f, 0.0f, 0.0f},
        {0.0073f, -0.2144f, 0.0093f, 0.6460f, -0.4668f, 0.0186f, 0.0f, 0.0f},
        {0.0068f, -0.1997f, -0.0288f, 0.6846f, -0.4819f, 0.0186f, 0.0f, 0.0f},
        {0.0063f, -0.1865f, -0.0649f, 0.7231f, -0.4966f, 0.0190f, 0.0f, 0.0f},
        {0.0059f, -0.1724f, -0.1021f, 0.7598f, -0.5107f, 0.0195f, 0.0f, 0.0f},
        {0.0054f, -0.1587f, -0.1367f, 0.7944f, -0.5239f, 0.0195f, 0.0f, 0.0f},
        {0.0049f, -0.1465f, -0.1709f, 0.8301f, -0.5371f, 0.0200f, 0.0f, 0.0f},
        {0.0044f, -0.1343f, -0.2046f, 0.8643f, -0.5498f, 0.0205f, 0.0f, 0.0f},
        {0.0039f, -0.1226f, -0.2368f, 0.8975f, -0.5620f, 0.0200f, 0.0f, 0.0f},
        {0.0034f, -0.1104f, -0.2690f, 0.9292f, -0.5728f, 0.0200f, 0.0f, 0.0f},
        {0.0029f, -0.1001f, -0.2988f, 0.9580f, -0.5820f, 0.0200f, 0.0f, 0.0f},
        {0.0024f, -0.0898f, -0.3286f, 0.9893f, -0.5928f, 0.0200f, 0.0f, 0.0f},
        {0.0020f, -0.0796f, -0.3574f, 1.0161f, -0.6006f, 0.0195f, 0.0f, 0.0f},
        {0.0015f, -0.0698f, -0.3843f, 1.0405f, -0.6069f, 0.0190f, 0.0f, 0.0f},
        {0.0015f, -0.0615f, -0.4102f, 1.0659f, -0.6138f, 0.0181f, 0.0f, 0.0f},
        {0.0010f, -0.0532f, -0.4346f, 1.0879f, -0.6187f, 0.0176f, 0.0f, 0.0f},
        {0.0010f, -0.0454f, -0.4570f, 1.1089f, -0.6235f, 0.0166f, 0.0f, 0.0f},
        {0.0005f, -0.0386f, -0.4790f, 1.1274f, -0.6265f, 0.0156f, 0.0f, 0.0f},
        {0.0005f, -0.0317f, -0.5005f, 1.1455f, -0.6284f, 0.0142f, 0.0f, 0.0f},
        {0.0005f, -0.0259f, -0.5181f, 1.1582f, -0.6270f, 0.0122f, 0.0f, 0.0f},
        {0.0005f, -0.0195f, -0.5352f, 1.1699f, -0.6265f, 0.0112f, 0.0f, 0.0f},
        {0.0f, -0.0151f, -0.5513f, 1.1802f, -0.6235f, 0.0093f, 0.0f, 0.0f},
        {0.0f, -0.0103f, -0.5659f, 1.1890f, -0.6196f, 0.0073f, 0.0f, 0.0f},
        {0.0005f, -0.0068f, -0.5791f, 1.1958f, -0.6147f, 0.0049f, 0.0f, 0.0f},
        {0.0f, -0.0029f, -0.5903f, 1.1987f, -0.6084f, 0.0029f, 0.0f, 0.0f}
    };
NVF4 GetEdgeMap(NVF p[6][6], NVI i, NVI j){
    const NVF g_0 = abs(p[0 + i][0 + j] + p[0 + i][1 + j] + p[0 + i][2 + j] - p[2 + i][0 + j] - p[2 + i][1 + j] - p[2 + i][2 + j]);
    const NVF g_45 = abs(p[1 + i][0 + j] + p[0 + i][0 + j] + p[0 + i][1 + j] - p[2 + i][1 + j] - p[2 + i][2 + j] - p[1 + i][2 + j]);
    const NVF g_90 = abs(p[0 + i][0 + j] + p[1 + i][0 + j] + p[2 + i][0 + j] - p[0 + i][2 + j] - p[1 + i][2 + j] - p[2 + i][2 + j]);
    const NVF g_135 = abs(p[1 + i][0 + j] + p[2 + i][0 + j] + p[2 + i][1 + j] - p[0 + i][1 + j] - p[0 + i][2 + j] - p[1 + i][2 + j]);

    const NVF g_0_90_max = max(g_0, g_90);
    const NVF g_0_90_min = min(g_0, g_90);
    const NVF g_45_135_max = max(g_45, g_135);
    const NVF g_45_135_min = min(g_45, g_135);

    NVF e_0_90 = 0;
    NVF e_45_135 = 0;

    if (g_0_90_max + g_45_135_max == 0)
    {
        return NVF4(0, 0, 0, 0);
    }

    e_0_90 = min(g_0_90_max / (g_0_90_max + g_45_135_max), 1.0f);
    e_45_135 = 1.0f - e_0_90;

    NVB c_0_90 = (g_0_90_max > (g_0_90_min * kDetectRatio)) && (g_0_90_max > kDetectThres) && (g_0_90_max > g_45_135_min);
    NVB c_45_135 = (g_45_135_max > (g_45_135_min * kDetectRatio)) && (g_45_135_max > kDetectThres) && (g_45_135_max > g_0_90_min);
    NVB c_g_0_90 = g_0_90_max == g_0;
    NVB c_g_45_135 = g_45_135_max == g_45;

    NVF f_e_0_90 = (c_0_90 && c_45_135) ? e_0_90 : 1.0f;
    NVF f_e_45_135 = (c_0_90 && c_45_135) ? e_45_135 : 1.0f;

    NVF weight_0 = (c_0_90 && c_g_0_90) ? f_e_0_90 : 0.0f;
    NVF weight_90 = (c_0_90 && !c_g_0_90) ? f_e_0_90 : 0.0f;
    NVF weight_45 = (c_45_135 && c_g_45_135) ? f_e_45_135 : 0.0f;
    NVF weight_135 = (c_45_135 && !c_g_45_135) ? f_e_45_135 : 0.0f;

    return NVF4(weight_0, weight_90, weight_45, weight_135);
}
NVF CalcLTI(NVF p0, NVF p1, NVF p2, NVF p3, NVF p4, NVF p5, NVI phase_index)
{
    const NVB selector = (phase_index <= kPhaseCount / 2);
    NVF sel = selector ? p0 : p3;
    const NVF a_min = min(min(p1, p2), sel);
    const NVF a_max = max(max(p1, p2), sel);
    sel = selector ? p2 : p5;
    const NVF b_min = min(min(p3, p4), sel);
    const NVF b_max = max(max(p3, p4), sel);

    const NVF a_cont = a_max - a_min;
    const NVF b_cont = b_max - b_min;

    const NVF cont_ratio = max(a_cont, b_cont) / (min(a_cont, b_cont) + kEps);
    return (1.0f - saturate((cont_ratio - kMinContrastRatio) * kRatioNorm)) * kContrastBoost;
}
NVF EvalPoly6(const NVF pxl[6], NVI phase_int)
{
    NVF y = 0.f;
    {
        NIS_UNROLL
        for (NVI i = 0; i < 6; ++i)
        {
            y += shCoefScaler[phase_int][i] * pxl[i];
        }
    }
    NVF y_usm = 0.f;
    {
        NIS_UNROLL
        for (NVI i = 0; i < 6; ++i)
        {
            y_usm += shCoefUSM[phase_int][i] * pxl[i];
        }
    }

    // let's compute a piece-wise ramp based on luma
    const NVF y_scale = 1.0f - saturate((y * (1.0f / NIS_SCALE_FLOAT) - kSharpStartY) * kSharpScaleY);

    // scale the ramp to sharpen as a function of luma
    const NVF y_sharpness = y_scale * kSharpStrengthScale + kSharpStrengthMin;

    y_usm *= y_sharpness;

    // scale the ramp to limit USM as a function of luma
    const NVF y_sharpness_limit = (y_scale * kSharpLimitScale + kSharpLimitMin) * y;

    y_usm = min(y_sharpness_limit, max(-y_sharpness_limit, y_usm));
    // reduce ringing
    y_usm *= CalcLTI(pxl[0], pxl[1], pxl[2], pxl[3], pxl[4], pxl[5], phase_int);

    return y + y_usm;
}
NVF FilterNormal(const NVF p[6][6], NVI phase_x_frac_int, NVI phase_y_frac_int)
{
    NVF h_acc = 0.0f;
    NIS_UNROLL
    for (NVI j = 0; j < 6; ++j)
    {
        NVF v_acc = 0.0f;
        NIS_UNROLL
        for (NVI i = 0; i < 6; ++i)
        {
            v_acc += p[i][j] * shCoefScaler[phase_y_frac_int][i];
        }
        h_acc += v_acc * shCoefScaler[phase_x_frac_int][j];
    }

    // let's return the sum unpacked -> we can accumulate it later
    return h_acc;
}
NVF AddDirFilters(NVF p[6][6], NVF phase_x_frac, NVF phase_y_frac, NVI phase_x_frac_int, NVI phase_y_frac_int, NVF4 w)
{
    NVF f = 0;
    if (w.x > 0.0f)
    {
        // 0 deg filter
        NVF interp0Deg[6];
        {
            NIS_UNROLL
                for (NVI i = 0; i < 6; ++i)
                {
                    interp0Deg[i] = lerp(p[i][2], p[i][3], phase_x_frac);
                }
        }
        f += EvalPoly6(interp0Deg, phase_y_frac_int) * w.x;
    }
    if (w.y > 0.0f)
    {
        // 90 deg filter
        NVF interp90Deg[6];
        {
            NIS_UNROLL
                for (NVI i = 0; i < 6; ++i)
                {
                    interp90Deg[i] = lerp(p[2][i], p[3][i], phase_y_frac);
                }
        }

        f += EvalPoly6(interp90Deg, phase_x_frac_int) * w.y;
    }
    if (w.z > 0.0f)
    {
        //45 deg filter
        NVF pphase_b45 = 0.5f + 0.5f * (phase_x_frac - phase_y_frac);

        NVF temp_interp45Deg[7];
        temp_interp45Deg[1] = lerp(p[2][1], p[1][2], pphase_b45);
        temp_interp45Deg[3] = lerp(p[3][2], p[2][3], pphase_b45);
        temp_interp45Deg[5] = lerp(p[4][3], p[3][4], pphase_b45);
        {
            pphase_b45 = pphase_b45 - 0.5f;
            NVF a = (pphase_b45 >= 0.f) ? p[0][2] : p[2][0];
            NVF b = (pphase_b45 >= 0.f) ? p[1][3] : p[3][1];
            NVF c = (pphase_b45 >= 0.f) ? p[2][4] : p[4][2];
            NVF d = (pphase_b45 >= 0.f) ? p[3][5] : p[5][3];
            temp_interp45Deg[0] = lerp(p[1][1], a, abs(pphase_b45));
            temp_interp45Deg[2] = lerp(p[2][2], b, abs(pphase_b45));
            temp_interp45Deg[4] = lerp(p[3][3], c, abs(pphase_b45));
            temp_interp45Deg[6] = lerp(p[4][4], d, abs(pphase_b45));
        }

        NVF interp45Deg[6];
        NVF pphase_p45 = phase_x_frac + phase_y_frac;
        if (pphase_p45 >= 1)
        {
            NIS_UNROLL
                for (NVI i = 0; i < 6; i++)
                {
                    interp45Deg[i] = temp_interp45Deg[i + 1];
                }
            pphase_p45 = pphase_p45 - 1;
        }
        else
        {
            NIS_UNROLL
                for (NVI i = 0; i < 6; i++)
                {
                    interp45Deg[i] = temp_interp45Deg[i];
                }
        }

        f += EvalPoly6(interp45Deg, NVI(pphase_p45 * 64)) * w.z;
    }
    if (w.w > 0.0f)
    {
        //135 deg filter
        NVF pphase_b135 = 0.5f * (phase_x_frac + phase_y_frac);

        NVF temp_interp135Deg[7];
        temp_interp135Deg[1] = lerp(p[3][1], p[4][2], pphase_b135);
        temp_interp135Deg[3] = lerp(p[2][2], p[3][3], pphase_b135);
        temp_interp135Deg[5] = lerp(p[1][3], p[2][4], pphase_b135);
        {
            pphase_b135 = pphase_b135 - 0.5f;
            NVF a = (pphase_b135 >= 0.f) ? p[5][2] : p[3][0];
            NVF b = (pphase_b135 >= 0.f) ? p[4][3] : p[2][1];
            NVF c = (pphase_b135 >= 0.f) ? p[3][4] : p[1][2];
            NVF d = (pphase_b135 >= 0.f) ? p[2][5] : p[0][3];
            temp_interp135Deg[0] = lerp(p[4][1], a, abs(pphase_b135));
            temp_interp135Deg[2] = lerp(p[3][2], b, abs(pphase_b135));
            temp_interp135Deg[4] = lerp(p[2][3], c, abs(pphase_b135));
            temp_interp135Deg[6] = lerp(p[1][4], d, abs(pphase_b135));
        }

        NVF interp135Deg[6];
        NVF pphase_p135 = 1 + (phase_x_frac - phase_y_frac);
        if (pphase_p135 >= 1)
        {
            NIS_UNROLL
                for (NVI i = 0; i < 6; ++i)
                {
                    interp135Deg[i] = temp_interp135Deg[i + 1];
                }
            pphase_p135 = pphase_p135 - 1;
        }
        else
        {
            NIS_UNROLL
                for (NVI i = 0; i < 6; ++i)
                {
                    interp135Deg[i] = temp_interp135Deg[i];
                }
        }

        f += EvalPoly6(interp135Deg, NVI(pphase_p135 * 64)) * w.w;
    }
    return f;
}
 void FsrEasuTapF(
 AF3 &aC, // Accumulated color, with negative lobe.
 AF1 &aW, // Accumulated weight.
 AF2 off, // Pixel offset from resolve position to tap.
 AF2 dir, // Gradient direction.
 AF2 len, // Length.
 AF1 lob, // Negative lobe strength.
 AF1 clp, // Clipping point.
 AF3 c){ // Tap color.
  // Rotate offset by direction.
  AF2 v;
  v.x=(off.x*( dir.x))+(off.y*dir.y);
  v.y=(off.x*(-dir.y))+(off.y*dir.x);
  // Anisotropy.
  v*=len;
  // Compute distance^2.
  AF1 d2=v.x*v.x+v.y*v.y;
  // Limit to the window as at corner, 2 taps can easily be outside.
  d2=min(d2,clp);
  // Approximation of lancos2 without sin() or rcp(), or sqrt() to get x.
  //  (25/16 * (2/5 * x^2 - 1)^2 - (25/16 - 1)) * (1/4 * x^2 - 1)^2
  //  |_______________________________________|   |_______________|
  //                   base                             window
  // The general form of the 'base' is,
  //  (a*(b*x^2-1)^2-(a-1))
  // Where 'a=1/(2*b-b^2)' and 'b' moves around the negative lobe.
  AF1 wB=AF1_(2.0/5.0)*d2+AF1_(-1.0);
  AF1 wA=lob*d2+AF1_(-1.0);
  wB*=wB;
  wA*=wA;
  wB=AF1_(25.0/16.0)*wB+AF1_(-(25.0/16.0-1.0));
  AF1 w=wB*wA;
  // Do weighted average.
  aC+=c*w;aW+=w;}
 void FsrEasuSetF(
 AF2 &dir,
 AF1 &len,
 AF2 pp,
 AP1 biS,AP1 biT,AP1 biU,AP1 biV,
 AF1 lA,AF1 lB,AF1 lC,AF1 lD,AF1 lE){
  // Compute bilinear weight, branches factor as &predicates are compiler time immediates.
  //  s t
  //  u v
  AF1 w = AF1_(0.0);
  if(biS)w=(AF1_(1.0)-pp.x)*(AF1_(1.0)-pp.y);
  if(biT)w=           pp.x *(AF1_(1.0)-pp.y);
  if(biU)w=(AF1_(1.0)-pp.x)*           pp.y ;
  if(biV)w=           pp.x *           pp.y ;
  // Direction is the '+' diff.
  //    a
  //  b c d
  //    e
  // Then takes magnitude from abs average of both sides of 'c'.
  // Length converts gradient reversal to 0, smoothly to non-reversal at 1, shaped, then adding horz and vert terms.
  AF1 dc=lD-lC;
  AF1 cb=lC-lB;
  AF1 lenX=max(abs(dc),abs(cb));
  lenX=APrxLoRcpF1(lenX);
  AF1 dirX=lD-lB;
  dir.x+=dirX*w;
  lenX=ASatF1(abs(dirX)*lenX);
  lenX*=lenX;
  len+=lenX*w;
  // Repeat for the y axis.
  AF1 ec=lE-lC;
  AF1 ca=lC-lA;
  AF1 lenY=max(abs(ec),abs(ca));
  lenY=APrxLoRcpF1(lenY);
  AF1 dirY=lE-lA;
  dir.y+=dirY*w;
  lenY=ASatF1(abs(dirY)*lenY);
  lenY*=lenY;
  len+=lenY*w;}
 void FsrEasuF(
 AF3 &pix,
 AU2 ip, // Integer pixel position in output.
 AU4 con0, // Constants generated by FsrEasuCon().
 AU4 con1,
 AU4 con2,
 AU4 con3){
//------------------------------------------------------------------------------------------------------------------------------
  // Get position of 'f'.
  AF2 pp=AF2(ip)*AF2_AU2(con0.xy())+AF2_AU2(con0.zw());
  AF2 fp=floor(pp);
  pp-=fp;
//------------------------------------------------------------------------------------------------------------------------------
  // 12-tap kernel.
  //    b c
  //  e f g h
  //  i j k l
  //    n o
  // Gather 4 ordering.
  //  a b
  //  r g
  // For packed FP16, need either {rg} or {ab} so using the following setup for gather in all versions,
  //    a b    <- unused (z)
  //    r g
  //  a b a b
  //  r g r g
  //    a b
  //    r g    <- unused (z)
  // Allowing dead-code removal to remove the 'z's.
  AF2 p0=fp*AF2_AU2(con1.xy())+AF2_AU2(con1.zw());
  // These are from p0 to avoid pulling two constants on pre-Navi hardware.
  AF2 p1=p0+AF2_AU2(con2.xy());
  AF2 p2=p0+AF2_AU2(con2.zw());
  AF2 p3=p0+AF2_AU2(con3.xy());
  AF4 bczzR=FsrEasuRF(p0);
  AF4 bczzG=FsrEasuGF(p0);
  AF4 bczzB=FsrEasuBF(p0);
  AF4 ijfeR=FsrEasuRF(p1);
  AF4 ijfeG=FsrEasuGF(p1);
  AF4 ijfeB=FsrEasuBF(p1);
  AF4 klhgR=FsrEasuRF(p2);
  AF4 klhgG=FsrEasuGF(p2);
  AF4 klhgB=FsrEasuBF(p2);
  AF4 zzonR=FsrEasuRF(p3);
  AF4 zzonG=FsrEasuGF(p3);
  AF4 zzonB=FsrEasuBF(p3);
//------------------------------------------------------------------------------------------------------------------------------
  // Simplest multi-channel approximate luma possible (luma times 2, in 2 FMA/MAD).
  AF4 bczzL=bczzB*AF4_(0.5)+(bczzR*AF4_(0.5)+bczzG);
  AF4 ijfeL=ijfeB*AF4_(0.5)+(ijfeR*AF4_(0.5)+ijfeG);
  AF4 klhgL=klhgB*AF4_(0.5)+(klhgR*AF4_(0.5)+klhgG);
  AF4 zzonL=zzonB*AF4_(0.5)+(zzonR*AF4_(0.5)+zzonG);
  // Rename.
  AF1 bL=bczzL.x;
  AF1 cL=bczzL.y;
  AF1 iL=ijfeL.x;
  AF1 jL=ijfeL.y;
  AF1 fL=ijfeL.z;
  AF1 eL=ijfeL.w;
  AF1 kL=klhgL.x;
  AF1 lL=klhgL.y;
  AF1 hL=klhgL.z;
  AF1 gL=klhgL.w;
  AF1 oL=zzonL.z;
  AF1 nL=zzonL.w;
  // Accumulate for bilinear interpolation.
  AF2 dir=AF2_(0.0);
  AF1 len=AF1_(0.0);
  FsrEasuSetF(dir,len,pp,true, false,false,false,bL,eL,fL,gL,jL);
  FsrEasuSetF(dir,len,pp,false,true ,false,false,cL,fL,gL,hL,kL);
  FsrEasuSetF(dir,len,pp,false,false,true ,false,fL,iL,jL,kL,nL);
  FsrEasuSetF(dir,len,pp,false,false,false,true ,gL,jL,kL,lL,oL);
//------------------------------------------------------------------------------------------------------------------------------
  // Normalize with approximation, and cleanup close to zero.
  AF2 dir2=dir*dir;
  AF1 dirR=dir2.x+dir2.y;
  AP1 zro=dirR<AF1_(1.0/32768.0);
  dirR=APrxLoRsqF1(dirR);
  dirR=zro?AF1_(1.0):dirR;
  dir.x=zro?AF1_(1.0):dir.x;
  dir*=AF2_(dirR);
  // Transform from {0 to 2} to {0 to 1} range, and shape with square.
  len=len*AF1_(0.5);
  len*=len;
  // Stretch kernel {1.0 vert|horz, to sqrt(2.0) on diagonal}.
  AF1 stretch=(dir.x*dir.x+dir.y*dir.y)*APrxLoRcpF1(max(abs(dir.x),abs(dir.y)));
  // Anisotropic length after rotation,
  //  x := 1.0 lerp to 'stretch' on edges
  //  y := 1.0 lerp to 2x on edges
  AF2 len2=AF2(AF1_(1.0)+(stretch-AF1_(1.0))*len,AF1_(1.0)+AF1_(-0.5)*len);
  // Based on the amount of 'edge',
  // the window shifts from +/-{sqrt(2.0) to slightly beyond 2.0}.
  AF1 lob=AF1_(0.5)+AF1_((1.0/4.0-0.04)-0.5)*len;
  // Set distance^2 clipping point to the end of the adjustable window.
  AF1 clp=APrxLoRcpF1(lob);
//------------------------------------------------------------------------------------------------------------------------------
  // Accumulation mixed with min/max of 4 nearest.
  //    b c
  //  e f g h
  //  i j k l
  //    n o
  AF3 min4=min(AMin3F3(AF3(ijfeR.z,ijfeG.z,ijfeB.z),AF3(klhgR.w,klhgG.w,klhgB.w),AF3(ijfeR.y,ijfeG.y,ijfeB.y)),
               AF3(klhgR.x,klhgG.x,klhgB.x));
  AF3 max4=max(AMax3F3(AF3(ijfeR.z,ijfeG.z,ijfeB.z),AF3(klhgR.w,klhgG.w,klhgB.w),AF3(ijfeR.y,ijfeG.y,ijfeB.y)),
               AF3(klhgR.x,klhgG.x,klhgB.x));
  // Accumulation.
  AF3 aC=AF3_(0.0);
  AF1 aW=AF1_(0.0);
  FsrEasuTapF(aC,aW,AF2( 0.0,-1.0)-pp,dir,len2,lob,clp,AF3(bczzR.x,bczzG.x,bczzB.x)); // b
  FsrEasuTapF(aC,aW,AF2( 1.0,-1.0)-pp,dir,len2,lob,clp,AF3(bczzR.y,bczzG.y,bczzB.y)); // c
  FsrEasuTapF(aC,aW,AF2(-1.0, 1.0)-pp,dir,len2,lob,clp,AF3(ijfeR.x,ijfeG.x,ijfeB.x)); // i
  FsrEasuTapF(aC,aW,AF2( 0.0, 1.0)-pp,dir,len2,lob,clp,AF3(ijfeR.y,ijfeG.y,ijfeB.y)); // j
  FsrEasuTapF(aC,aW,AF2( 0.0, 0.0)-pp,dir,len2,lob,clp,AF3(ijfeR.z,ijfeG.z,ijfeB.z)); // f
  FsrEasuTapF(aC,aW,AF2(-1.0, 0.0)-pp,dir,len2,lob,clp,AF3(ijfeR.w,ijfeG.w,ijfeB.w)); // e
  FsrEasuTapF(aC,aW,AF2( 1.0, 1.0)-pp,dir,len2,lob,clp,AF3(klhgR.x,klhgG.x,klhgB.x)); // k
  FsrEasuTapF(aC,aW,AF2( 2.0, 1.0)-pp,dir,len2,lob,clp,AF3(klhgR.y,klhgG.y,klhgB.y)); // l
  FsrEasuTapF(aC,aW,AF2( 2.0, 0.0)-pp,dir,len2,lob,clp,AF3(klhgR.z,klhgG.z,klhgB.z)); // h
  FsrEasuTapF(aC,aW,AF2( 1.0, 0.0)-pp,dir,len2,lob,clp,AF3(klhgR.w,klhgG.w,klhgB.w)); // g
  FsrEasuTapF(aC,aW,AF2( 1.0, 2.0)-pp,dir,len2,lob,clp,AF3(zzonR.z,zzonG.z,zzonB.z)); // o
  FsrEasuTapF(aC,aW,AF2( 0.0, 2.0)-pp,dir,len2,lob,clp,AF3(zzonR.w,zzonG.w,zzonB.w)); // n
//------------------------------------------------------------------------------------------------------------------------------
  // Normalize and dering.
  pix=min(max4,max(min4,aC*AF3_(ARcpF1(aW))));}
 void FsrRcasF(
 AF1 &pixR, // Output values, non-vector so port between RcasFilter() and RcasFilterH() is easy.
 AF1 &pixG,
 AF1 &pixB,
 #ifdef FSR_RCAS_PASSTHROUGH_ALPHA
  AF1 &pixA,
 #endif
 AU2 ip, // Integer pixel position in output.
 AU4 con){ // Constant generated by RcasSetup().
  // Algorithm uses minimal 3x3 pixel neighborhood.
  //    b 
  //  d e f
  //    h
  ASU2 sp=ASU2(ip);
  AF3 b=FsrRcasLoadF(sp+ASU2( 0,-1)).rgb();
  AF3 d=FsrRcasLoadF(sp+ASU2(-1, 0)).rgb();
  #ifdef FSR_RCAS_PASSTHROUGH_ALPHA
   AF4 ee=FsrRcasLoadF(sp);
   AF3 e=ee.rgb();pixA=ee.a;
  #else
   AF3 e=FsrRcasLoadF(sp).rgb();
  #endif
  AF3 f=FsrRcasLoadF(sp+ASU2( 1, 0)).rgb();
  AF3 h=FsrRcasLoadF(sp+ASU2( 0, 1)).rgb();
  // Rename (32-bit) or regroup (16-bit).
  AF1 bR=b.r;
  AF1 bG=b.g;
  AF1 bB=b.b;
  AF1 dR=d.r;
  AF1 dG=d.g;
  AF1 dB=d.b;
  AF1 eR=e.r;
  AF1 eG=e.g;
  AF1 eB=e.b;
  AF1 fR=f.r;
  AF1 fG=f.g;
  AF1 fB=f.b;
  AF1 hR=h.r;
  AF1 hG=h.g;
  AF1 hB=h.b;
  // Run optional input transform.
  FsrRcasInputF(bR,bG,bB);
  FsrRcasInputF(dR,dG,dB);
  FsrRcasInputF(eR,eG,eB);
  FsrRcasInputF(fR,fG,fB);
  FsrRcasInputF(hR,hG,hB);
  // Luma times 2.
  AF1 bL=bB*AF1_(0.5)+(bR*AF1_(0.5)+bG);
  AF1 dL=dB*AF1_(0.5)+(dR*AF1_(0.5)+dG);
  AF1 eL=eB*AF1_(0.5)+(eR*AF1_(0.5)+eG);
  AF1 fL=fB*AF1_(0.5)+(fR*AF1_(0.5)+fG);
  AF1 hL=hB*AF1_(0.5)+(hR*AF1_(0.5)+hG);
  // Noise detection.
  AF1 nz=AF1_(0.25)*bL+AF1_(0.25)*dL+AF1_(0.25)*fL+AF1_(0.25)*hL-eL;
  nz=ASatF1(abs(nz)*APrxMedRcpF1(AMax3F1(AMax3F1(bL,dL,eL),fL,hL)-AMin3F1(AMin3F1(bL,dL,eL),fL,hL)));
  nz=AF1_(-0.5)*nz+AF1_(1.0);
  // Min and max of ring.
  AF1 mn4R=min(AMin3F1(bR,dR,fR),hR);
  AF1 mn4G=min(AMin3F1(bG,dG,fG),hG);
  AF1 mn4B=min(AMin3F1(bB,dB,fB),hB);
  AF1 mx4R=max(AMax3F1(bR,dR,fR),hR);
  AF1 mx4G=max(AMax3F1(bG,dG,fG),hG);
  AF1 mx4B=max(AMax3F1(bB,dB,fB),hB);
  // Immediate constants for peak range.
  AF2 peakC=AF2(1.0,-1.0*4.0);
  // Limiters, these need to be high precision RCPs.
  AF1 hitMinR=min(mn4R,eR)*ARcpF1(AF1_(4.0)*mx4R);
  AF1 hitMinG=min(mn4G,eG)*ARcpF1(AF1_(4.0)*mx4G);
  AF1 hitMinB=min(mn4B,eB)*ARcpF1(AF1_(4.0)*mx4B);
  AF1 hitMaxR=(peakC.x-max(mx4R,eR))*ARcpF1(AF1_(4.0)*mn4R+peakC.y);
  AF1 hitMaxG=(peakC.x-max(mx4G,eG))*ARcpF1(AF1_(4.0)*mn4G+peakC.y);
  AF1 hitMaxB=(peakC.x-max(mx4B,eB))*ARcpF1(AF1_(4.0)*mn4B+peakC.y);
  AF1 lobeR=max(-hitMinR,hitMaxR);
  AF1 lobeG=max(-hitMinG,hitMaxG);
  AF1 lobeB=max(-hitMinB,hitMaxB);
  AF1 lobe=max(AF1_(-FSR_RCAS_LIMIT),min(AMax3F1(lobeR,lobeG,lobeB),AF1_(0.0)))*AF1_AU1(con.x);
  // Apply noise removal.
  #ifdef FSR_RCAS_DENOISE
   lobe*=nz;
  #endif
  // Resolve, which needs the medium precision rcp approximation to avoid visible tonality changes.
  AF1 rcpL=APrxMedRcpF1(AF1_(4.0)*lobe+AF1_(1.0));
  pixR=(lobe*bR+lobe*dR+lobe*hR+lobe*fR+eR)*rcpL;
  pixG=(lobe*bG+lobe*dG+lobe*hG+lobe*fG+eG)*rcpL;
  pixB=(lobe*bB+lobe*dB+lobe*hB+lobe*fB+eB)*rcpL;
  return;}

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
