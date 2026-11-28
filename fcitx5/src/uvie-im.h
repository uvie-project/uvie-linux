/*
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * fcitx5-uvie — UVie Vietnamese input engine for Fcitx5.
 * Thin shell around the shared host dispatcher (uvie-host via
 * uvie_dispatch_*): a `preedit` string mirrors the dispatcher's output
 * plan; text is only committed at word boundaries.
 */
#ifndef _FCITX5_UVIE_UVIE_IM_H_
#define _FCITX5_UVIE_UVIE_IM_H_

#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>
#include <fcitx/addoninstance.h>
#include <fcitx/inputcontextproperty.h>
#include <fcitx/inputmethodengine.h>
#include <fcitx/instance.h>

#include <string>
#include <unordered_map>

#include "uvie.h"
#include "uvie-config.h"

namespace fcitx {

class UvieState;

class UvieEngine final : public InputMethodEngine {
public:
    UvieEngine(Instance *instance);

    void activate(const InputMethodEntry &entry,
                  InputContextEvent &event) override;
    void keyEvent(const InputMethodEntry &entry, KeyEvent &keyEvent) override;
    void reset(const InputMethodEntry &entry,
               InputContextEvent &event) override;
    void deactivate(const InputMethodEntry &entry,
                    InputContextEvent &event) override;

    auto &factory() { return factory_; }
    Instance *instance() { return instance_; }
    UvieSettings &config() { return config_; }
    const UvieSettings &config() const { return config_; }

    // `language` uses the uvie_dispatch ABI encoding: 0 Vietnamese,
    // 1 English. The app map is process-local memory; Fcitx input
    // contexts already preserve per-app state while they live, so this
    // covers newly-created contexts without adding another config file.
    void noteLanguage(const std::string &app, int language) {
        global_language_ = language;
        if (config_.per_app_language && !app.empty()) {
            app_languages_[app] = language;
        }
    }

    int languageFor(const std::string &app, bool perApp) const {
        if (!perApp) {
            return global_language_;
        }
        auto it = app_languages_.find(app);
        return it == app_languages_.end() ? -1 : it->second;
    }

private:
    Instance *instance_;
    FactoryFor<UvieState> factory_;
    UvieSettings config_{};
    int global_language_ = 0;
    std::unordered_map<std::string, int> app_languages_;
};

class UvieFactory : public AddonFactory {
public:
    AddonInstance *create(AddonManager *manager) override {
        return new UvieEngine(manager->instance());
    }
};

} // namespace fcitx

#endif // _FCITX5_UVIE_UVIE_IM_H_
