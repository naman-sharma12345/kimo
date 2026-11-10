// Native OCIO CPU adapter. No global/current config or environment mutation.
#include <OpenColorIO/OpenColorIO.h>
#if OCIO_VERSION_MAJOR != 2
#error "Kimo native OCIO adapter requires OpenColorIO 2"
#endif
#include <sstream>
#include <string>
#include <vector>
#include <memory>
#include <cstring>
#include <stdexcept>
namespace OCIO = OCIO_NAMESPACE;
struct KimoConfig { OCIO::ConstConfigRcPtr config; std::vector<std::string> spaces, displays, views; };
struct KimoProcessor { OCIO::ConstCPUProcessorRcPtr cpu; };
static void error(char *out, size_t cap, const char *message) noexcept {
    if (cap) { std::strncpy(out, message, cap - 1); out[cap - 1] = 0; }
}
static void check_transform(const OCIO::ConstTransformRcPtr &t, unsigned depth=0) {
    if (!t) return;
    if (depth > 32) throw std::runtime_error("OCIO transform nesting limit");
    if (t->getTransformType() == OCIO::TRANSFORM_TYPE_FILE)
        throw std::runtime_error("external FileTransform disabled: self-contained configs only");
    auto group = OCIO_DYNAMIC_POINTER_CAST<const OCIO::GroupTransform>(t);
    if (group) {
        if (group->getNumTransforms() > 4096) throw std::runtime_error("OCIO group size limit");
        for (int i=0;i<group->getNumTransforms();++i) check_transform(group->getTransform(i), depth+1);
    }
}
extern "C" {
const char *kimo_ocio_version() noexcept { return OCIO::GetVersion(); }
void *kimo_ocio_load(const char *text, char *err, size_t cap) noexcept {
    try {
        std::istringstream stream(text);
        auto c = OCIO::Config::CreateFromStream(stream)->createEditableCopy();
        c->clearEnvironmentVars(); c->setSearchPath(""); c->setWorkingDir("");
        // Listing must not depend on OCIO_ACTIVE_DISPLAYS/VIEWS in the process environment.
        c->setActiveDisplays(""); c->setActiveViews("");
        int count = c->getNumColorSpaces(OCIO::SEARCH_REFERENCE_SPACE_ALL, OCIO::COLORSPACE_ALL);
        if (count > 4096 || c->getNumLooks()>4096 || c->getNumNamedTransforms(OCIO::NAMEDTRANSFORM_ALL)>4096 || c->getNumViewTransforms()>4096)
            throw std::runtime_error("OCIO catalog size limit");
        auto result = std::make_unique<KimoConfig>();
        for (int i=0;i<count;++i) {
            auto name=c->getColorSpaceNameByIndex(OCIO::SEARCH_REFERENCE_SPACE_ALL, OCIO::COLORSPACE_ALL, i);
            auto space=c->getColorSpace(name);
            check_transform(space->getTransform(OCIO::COLORSPACE_DIR_TO_REFERENCE));
            check_transform(space->getTransform(OCIO::COLORSPACE_DIR_FROM_REFERENCE));
            result->spaces.emplace_back(name);
        }
        for (int i=0;i<c->getNumLooks();++i) {
            auto look=c->getLook(c->getLookNameByIndex(i));
            check_transform(look->getTransform()); check_transform(look->getInverseTransform());
        }
        for (int i=0;i<c->getNumNamedTransforms(OCIO::NAMEDTRANSFORM_ALL);++i) {
            auto t=c->getNamedTransform(c->getNamedTransformNameByIndex(OCIO::NAMEDTRANSFORM_ALL, i));
            check_transform(t->getTransform(OCIO::TRANSFORM_DIR_FORWARD));
            check_transform(t->getTransform(OCIO::TRANSFORM_DIR_INVERSE));
        }
        for (int i=0;i<c->getNumViewTransforms();++i) {
            auto t=c->getViewTransform(c->getViewTransformNameByIndex(i));
            check_transform(t->getTransform(OCIO::VIEWTRANSFORM_DIR_TO_REFERENCE));
            check_transform(t->getTransform(OCIO::VIEWTRANSFORM_DIR_FROM_REFERENCE));
        }
        c->validate();
        if(c->getNumDisplaysAll()>4096) throw std::runtime_error("OCIO display count limit");
        for (int i=0;i<c->getNumDisplaysAll();++i) {
            const char *d=c->getDisplayAll(i);
            if(c->getNumViews(d)>4096) throw std::runtime_error("OCIO view count limit");
            for(int j=0;j<c->getNumViews(d);++j) {
                result->displays.emplace_back(d); result->views.emplace_back(c->getView(d,j));
            }
        }
        result->config=c; return result.release();
    } catch(const std::exception &e) { error(err,cap,e.what()); }
      catch(...) { error(err,cap,"unknown OCIO load error"); }
    return nullptr;
}
void kimo_ocio_config_free(void *p) noexcept { delete static_cast<KimoConfig*>(p); }
size_t kimo_ocio_count(const void *p, int catalog) noexcept {
    auto c=static_cast<const KimoConfig*>(p); return catalog==0 ? c->spaces.size() : c->views.size();
}
const char *kimo_ocio_name(const void *p, int catalog, size_t i) noexcept {
    auto c=static_cast<const KimoConfig*>(p);
    auto &v=catalog==0 ? c->spaces : catalog==1 ? c->displays : c->views;
    return i<v.size() ? v[i].c_str() : "";
}
void *kimo_ocio_processor(const void *p, const char *src, const char *dst, char *err, size_t cap) noexcept {
    try {
        auto c=static_cast<const KimoConfig*>(p);
        if(!c->config->getColorSpace(src) || !c->config->getColorSpace(dst))
            throw std::runtime_error("unknown OCIO colorspace");
        auto result=std::make_unique<KimoProcessor>();
        // Disable approximate optimized CPU math for reproducible reference comparisons.
        result->cpu=c->config->getProcessor(src,dst)->getOptimizedCPUProcessor(OCIO::OPTIMIZATION_NONE);
        return result.release();
    } catch(const std::exception &e) { error(err,cap,e.what()); }
      catch(...) { error(err,cap,"unknown OCIO processor error"); }
    return nullptr;
}
void *kimo_ocio_display_processor(const void *p, const char *src, const char *display, const char *view, char *err, size_t cap) noexcept {
    try {
        auto c=static_cast<const KimoConfig*>(p);
        if(!c->config->getColorSpace(src)) throw std::runtime_error("unknown OCIO source colorspace");
        bool found=false;
        for(int i=0;i<c->config->getNumDisplaysAll();++i) {
            const char *d=c->config->getDisplayAll(i);
            if(std::strcmp(d,display)!=0) continue;
            for(int j=0;j<c->config->getNumViews(d);++j)
                if(std::strcmp(c->config->getView(d,j),view)==0) found=true;
        }
        if(!found) throw std::runtime_error("unknown OCIO display/view pair");
        auto transform=OCIO::DisplayViewTransform::Create();
        transform->setSrc(src); transform->setDisplay(display); transform->setView(view);
        transform->setLooksBypass(false);
        auto result=std::make_unique<KimoProcessor>();
        result->cpu=c->config->getProcessor(transform)->getOptimizedCPUProcessor(OCIO::OPTIMIZATION_NONE);
        return result.release();
    } catch(const std::exception &e) { error(err,cap,e.what()); }
      catch(...) { error(err,cap,"unknown OCIO display processor error"); }
    return nullptr;
}
void *kimo_ocio_cdl(const double *slope,const double *offset,const double *power,double sat,char *err,size_t cap) noexcept {
 try{auto t=OCIO::CDLTransform::Create();t->setSlope(slope);t->setOffset(offset);t->setPower(power);t->setSat(sat);t->setStyle(OCIO::CDL_ASC);auto r=std::make_unique<KimoProcessor>();r->cpu=OCIO::Config::CreateRaw()->getProcessor(t)->getOptimizedCPUProcessor(OCIO::OPTIMIZATION_NONE);return r.release();}
 catch(const std::exception &e){error(err,cap,e.what());}catch(...){error(err,cap,"unknown nativeCDLerror");}return nullptr;
}
void kimo_ocio_processor_free(void *p) noexcept { delete static_cast<KimoProcessor*>(p); }
int kimo_ocio_noop(const void *p) noexcept { return static_cast<const KimoProcessor*>(p)->cpu->isNoOp(); }
int kimo_ocio_apply(const void *p, float *rgba, size_t pixels, char *err, size_t cap) noexcept {
    try {
        auto cpu=static_cast<const KimoProcessor*>(p)->cpu;
        // RGB-only: alpha is bit-preserved, even if a config embeds alpha matrix terms.
        if(!cpu->isNoOp()) for(size_t i=0;i<pixels;++i) cpu->applyRGB(rgba+4*i);
        return 1;
    } catch(const std::exception &e) { error(err,cap,e.what()); }
      catch(...) { error(err,cap,"unknown OCIO apply error"); }
    return 0;
}
}
