/*
 * fcitx5-uvie — UVie Vietnamese input engine for Fcitx5.
 * Thin shell around the shared host dispatcher (uvie-host via
 * uvie_dispatch_*): a `preedit` string mirrors the dispatcher's output
 * plan; text is only committed at word boundaries.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#include "uvie-im.h"
#include <fcitx-utils/keysym.h>

#include <fcitx-utils/utf8.h>
#include <fcitx/action.h>
#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>

#include "uvie-config.h"

namespace fcitx {

namespace {

Text preeditText(const std::string &s) {
    Text text;
    text.append(s, TextFormatFlag::Underline);
    text.setCursor(utf8::lengthValidated(s));
    return text;
}

/* Classify a keysym into the dispatcher's KeyKind + resolved codepoint.
 * Keysyms <= 0xFDFF are the unicode value for printable keys; the
 * 0xFE00-0xFFFF range holds dead keys and function keysyms, which fall
 * through to Other (pass-through). */
int classifySym(FcitxKeySym sym, unsigned int *ch) {
    *ch = 0;
    switch (sym) {
    case FcitxKey_BackSpace:
        return UVIE_KIND_BACKSPACE;
    case FcitxKey_Return:
    case FcitxKey_KP_Enter:
        return UVIE_KIND_ENTER;
    case FcitxKey_Tab:
    case FcitxKey_KP_Tab:
    case FcitxKey_ISO_Left_Tab:
        return UVIE_KIND_TAB;
    case FcitxKey_Escape:
        return UVIE_KIND_ESCAPE;
    case FcitxKey_Left:
    case FcitxKey_KP_Left:
        return UVIE_KIND_LEFT;
    case FcitxKey_Right:
    case FcitxKey_KP_Right:
        return UVIE_KIND_RIGHT;
    case FcitxKey_Delete:
    case FcitxKey_KP_Delete:
        return UVIE_KIND_FORWARD_DELETE;
    }
    // Home/End/Begin/Up/Down/Page/Insert (Left/Right, KP_Delete taken
    // above; KP_Delete slots into the range only after the switch).
    if ((sym >= FcitxKey_Home && sym <= FcitxKey_Insert) ||
        (sym >= FcitxKey_KP_Home && sym <= FcitxKey_KP_Insert)) {
        return UVIE_KIND_NAVIGATION;
    }
    // Numpad: digits feed the engine like their ASCII twins (VNI uses
    // them); the operators map to their ASCII equivalents so they still
    // commit the word.
    if (sym >= FcitxKey_KP_0 && sym <= FcitxKey_KP_9) {
        *ch = '0' + (sym - FcitxKey_KP_0);
        return UVIE_KIND_CHAR;
    }
    switch (sym) {
    case FcitxKey_KP_Space:
        *ch = ' ';
        return UVIE_KIND_CHAR;
    case FcitxKey_KP_Add:
        *ch = '+';
        return UVIE_KIND_CHAR;
    case FcitxKey_KP_Subtract:
        *ch = '-';
        return UVIE_KIND_CHAR;
    case FcitxKey_KP_Multiply:
        *ch = '*';
        return UVIE_KIND_CHAR;
    case FcitxKey_KP_Divide:
        *ch = '/';
        return UVIE_KIND_CHAR;
    case FcitxKey_KP_Decimal:
        *ch = '.';
        return UVIE_KIND_CHAR;
    case FcitxKey_KP_Equal:
        *ch = '=';
        return UVIE_KIND_CHAR;
    }
    if (sym >= 0x20 && sym <= 0xFDFF) {
        *ch = sym;
        return UVIE_KIND_CHAR;
    }
    return UVIE_KIND_OTHER;
}

unsigned int modsFlags(const KeyStates &states) {
    unsigned int m = 0;
    if (states.test(KeyState::Ctrl))
        m |= UVIE_MOD_CTRL;
    if (states.test(KeyState::Alt))
        m |= UVIE_MOD_ALT;
    if (states.test(KeyState::Shift))
        m |= UVIE_MOD_SHIFT;
    if (states.test(KeyState::Super))
        m |= UVIE_MOD_WIN;
    return m;
}

} // namespace

class UvieState final : public InputContextProperty {
public:
    UvieState(UvieEngine *engine, InputContext *ic)
        : engine_(engine), ic_(ic), uvie_(uvie_dispatch_new()) {
        uvie_config_apply_dispatch(uvie_, &engine_->config());
    }
    ~UvieState() override {
        if (uvie_) {
            uvie_dispatch_free(uvie_);
        }
    }

    void keyEvent(KeyEvent &keyEvent) {
        // Releases are host bookkeeping — only presses feed the
        // dispatcher (ibus does the same via IBUS_RELEASE_MASK).
        if (keyEvent.isRelease()) {
            return;
        }
        const auto &cfg = engine_->config();

        // Master toggle off: finalize pending text and pass through.
        if (!cfg.enabled) {
            commitPending();
            return;
        }

        // Vi/En toggle chord — swallowed, like the daemon's hotkey.
        if (isToggleChord(keyEvent)) {
            commitPending();
            uvie_dispatch_toggle_language(uvie_);
            keyEvent.filterAndAccept();
            return;
        }

        // Hard modifiers: commit pending text, let the accelerator through.
        if (keyEvent.rawKey().states().testAny(KeyState::Ctrl_Alt) ||
            keyEvent.rawKey().states().test(KeyState::Super)) {
            commitPending();
            return;
        }

        auto sym = keyEvent.rawKey().sym();
        // Backspace with nothing mirrored: the app deletes committed text
        // natively — drop the engine's view of it.
        if (sym == FcitxKey_BackSpace && preedit_.empty() &&
            !uvie_dispatch_is_composing(uvie_)) {
            return;
        }

        unsigned int ch = 0;
        int kind = classifySym(sym, &ch);
        unsigned int ctx = 0;
        if (uvie_cfg_app_excluded(&cfg, ic_->program().c_str())) {
            ctx |= UVIE_CTX_APP_EXCLUDED;
        }
        UvieAction actions[16];
        int n = 16;
        int consumed =
            uvie_dispatch_feed(uvie_, kind, ch, modsFlags(keyEvent.rawKey().states()),
                               ctx, actions, &n);
        for (int i = 0; i < n; i++) {
            applyAction(actions[i]);
        }
        // A finished word (dispatcher no longer composing) is final
        // output — commit it to the app, consumed key or not.
        if (!uvie_dispatch_is_composing(uvie_)) {
            if (!preedit_.empty()) {
                ic_->commitString(preedit_);
                preedit_.clear();
            }
            clearPreedit();
        }
        updatePreedit();
        if (consumed) {
            keyEvent.filterAndAccept();
        }
    }

    void commitPending() {
        if (!preedit_.empty()) {
            ic_->commitString(preedit_);
            preedit_.clear();
        }
        uvie_dispatch_reset(uvie_);
        clearPreedit();
    }

    // Re-apply settings (and the macro table) — called on activation so
    // uvie-ui changes take effect without an engine restart.
    void applyConfig(const UvieSettings &cfg) {
        uvie_config_apply_dispatch(uvie_, &cfg);
    }

private:
    // The Vi/En toggle chord from settings.json — exact modifiers only;
    // an extra Super held means the chord belongs to the WM.
    bool isToggleChord(const KeyEvent &keyEvent) {
        const auto &cfg = engine_->config();
        if (cfg.hotkey_key <= 0) {
            return false;
        }
        auto raw = keyEvent.rawKey();
        // Key code encoding is frontend-dependent (xkb = evdev+8 on X11,
        // raw evdev possible elsewhere) — accept both. Trade-off: the
        // unused encoding can shadow a sibling physical key's chord.
        if (raw.code() != static_cast<unsigned>(cfg.hotkey_key) &&
            raw.code() != static_cast<unsigned>(cfg.hotkey_key) + 8) {
            return false;
        }
        auto s = raw.states();
        return s.test(KeyState::Ctrl) == !!cfg.hotkey_ctrl &&
               s.test(KeyState::Alt) == !!cfg.hotkey_alt &&
               s.test(KeyState::Shift) == !!cfg.hotkey_shift &&
               !s.test(KeyState::Super);
    }

    // Apply one plan action onto the preedit buffer. ForwardDelete
    // (type 1) never arrives: post-commit editing is off on the
    // preedit-model engines (see uvie-config.h).
    void applyAction(const UvieAction &a) {
        if (a.type == 0) {
            int n = a.n;
            while (n-- > 0 && !preedit_.empty()) {
                // erase one trailing UTF-8 char (continuation bytes first)
                while (!preedit_.empty() &&
                       (preedit_.back() & 0xC0) == 0x80) {
                    preedit_.pop_back();
                }
                if (!preedit_.empty()) {
                    preedit_.pop_back();
                }
            }
        } else if (a.type == 2 && a.text[0]) {
            preedit_.append(a.text);
        }
    }

    void updatePreedit() {
        auto &inputPanel = ic_->inputPanel();
        inputPanel.reset();
        if (preedit_.empty()) {
            clearPreedit();
            return;
        }
        inputPanel.setClientPreedit(preeditText(preedit_));
        ic_->updatePreedit();
        ic_->updateUserInterface(UserInterfaceComponent::InputPanel);
    }

    void clearPreedit() {
        auto &inputPanel = ic_->inputPanel();
        inputPanel.setClientPreedit(Text());
        ic_->updatePreedit();
        ic_->updateUserInterface(UserInterfaceComponent::InputPanel);
    }

    UvieEngine *engine_;
    InputContext *ic_;
    ::UvieDispatcher *uvie_ = nullptr;
    std::string preedit_;
};

UvieEngine::UvieEngine(Instance *instance)
    : instance_(instance), factory_([this](InputContext &ic) {
          return new UvieState(this, &ic);
      }) {
    // A state created before the first activate() must not see a
    // zero-initialized config (enabled=0 would pass everything through).
    uvie_config_load(&config_);
    instance_->inputContextManager().registerProperty("uvie-state",
                                                      &factory_);
}

void UvieEngine::keyEvent(const InputMethodEntry & /*entry*/,
                          KeyEvent &keyEvent) {
    auto *ic = keyEvent.inputContext();
    auto *state = ic->propertyFor(&factory_);
    state->keyEvent(keyEvent);
}

void UvieEngine::activate(const InputMethodEntry & /*entry*/,
                          InputContextEvent &event) {
    // Re-read settings on activation — uvie-ui may have changed them.
    uvie_config_load(&config_);
    // Re-apply to the focused context's dispatcher too, so changes take
    // effect without waiting for the app to lose focus.
    auto *state = event.inputContext()->propertyFor(&factory_);
    state->applyConfig(config_);
}

void UvieEngine::reset(const InputMethodEntry & /*entry*/,
                       InputContextEvent &event) {
    auto *state = event.inputContext()->propertyFor(&factory_);
    state->commitPending();
}

void UvieEngine::deactivate(const InputMethodEntry &entry,
                            InputContextEvent &event) {
    reset(entry, event);
}

} // namespace fcitx

FCITX_ADDON_FACTORY(fcitx::UvieFactory)
