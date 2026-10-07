/*
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * fcitx5-uvie — UVie Vietnamese input engine for Fcitx5.
 * Thin shell around libuvie (Rust engine, uvie.h), diff-driven like the
 * IBus engine: a `preedit` string mirrors engine output; text is only
 * committed at word boundaries.
 */
#ifndef _FCITX5_UVIE_UVIE_IM_H_
#define _FCITX5_UVIE_UVIE_IM_H_

#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>
#include <fcitx/addoninstance.h>
#include <fcitx/inputcontextproperty.h>
#include <fcitx/inputmethodengine.h>
#include <fcitx/instance.h>

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
    const UvieSettings &config() const { return config_; }

private:
    Instance *instance_;
    FactoryFor<UvieState> factory_;
    UvieSettings config_{};
};

class UvieFactory : public AddonFactory {
public:
    AddonInstance *create(AddonManager *manager) override {
        return new UvieEngine(manager->instance());
    }
};

} // namespace fcitx

#endif // _FCITX5_UVIE_UVIE_IM_H_
