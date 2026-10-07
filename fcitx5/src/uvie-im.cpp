/*
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

} // namespace

class UvieState final : public InputContextProperty {
public:
    UvieState(UvieEngine *engine, InputContext *ic)
        : engine_(engine), ic_(ic), uvie_(uvie_engine_new()) {
        uvie_config_apply(uvie_, &engine_->config());
    }
    ~UvieState() override {
        if (uvie_) {
            uvie_engine_free(uvie_);
        }
    }

    void keyEvent(KeyEvent &keyEvent) {
        char out[512];

        // Hard modifiers: commit pending text, let the accelerator through.
        if (keyEvent.rawKey().states().testAny(KeyState::Ctrl_Alt) ||
            keyEvent.rawKey().states().test(KeyState::Super)) {
            commitPending();
            return;
        }

        auto sym = keyEvent.rawKey().sym();
        if (sym == FcitxKey_BackSpace) {
            if (preedit_.empty()) {
                return;
            }
            size_t bs = uvie_engine_backspace(uvie_, out, sizeof(out));
            applyDiff(bs, out);
            updatePreedit();
            keyEvent.filterAndAccept();
            return;
        }

        // Navigation / break keys: commit, pass through.
        if ((sym >= FcitxKey_Home && sym <= FcitxKey_Insert) ||
            sym == FcitxKey_Delete || sym == FcitxKey_Return ||
            sym == FcitxKey_KP_Enter || sym == FcitxKey_Tab ||
            sym == FcitxKey_Escape) {
            commitPending();
            return;
        }

        // Space: commit the word, let the space itself through.
        if (sym == FcitxKey_space || sym == FcitxKey_KP_Space) {
            size_t bs = uvie_engine_commit(uvie_, out, sizeof(out));
            applyDiff(bs, out);
            commitPending();
            return;
        }

        // Printable ASCII → engine.
        if (sym >= FcitxKey_space && sym <= FcitxKey_asciitilde) {
            size_t bs = uvie_engine_feed(uvie_, static_cast<char>(sym), out,
                                       sizeof(out));
            applyDiff(bs, out);
            flushCommitted();
            updatePreedit();
            keyEvent.filterAndAccept();
            return;
        }

        commitPending();
    }

    void commitPending() {
        flushCommitted();
        if (!preedit_.empty()) {
            ic_->commitString(preedit_);
            preedit_.clear();
        }
        uvie_engine_reset(uvie_);
        flushed_ = 0;
        clearPreedit();
    }

private:
    void applyDiff(size_t backspaces, const char *suffix) {
        while (backspaces-- > 0 && !preedit_.empty()) {
            // erase one trailing UTF-8 char (continuation bytes first)
            while (!preedit_.empty() &&
                   (preedit_.back() & 0xC0) == 0x80) {
                preedit_.pop_back();
            }
            if (!preedit_.empty()) {
                preedit_.pop_back();
            }
        }
        if (suffix && *suffix) {
            preedit_.append(suffix);
        }
    }

    void flushCommitted() {
        char buf[512];
        size_t total =
            uvie_engine_committed_text(uvie_, buf, sizeof(buf));
        if (total <= flushed_) {
            return;
        }
        ic_->commitString(std::string(buf + flushed_, total - flushed_));
        preedit_.erase(0, total - flushed_);
        flushed_ = total;
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
    ::UvieEngine *uvie_ = nullptr;
    std::string preedit_;
    size_t flushed_ = 0;
};

UvieEngine::UvieEngine(Instance *instance)
    : instance_(instance), factory_([this](InputContext &ic) {
          return new UvieState(this, &ic);
      }) {
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
                          InputContextEvent & /*event*/) {
    // Re-read settings on activation — uvie-ui may have changed them.
    uvie_config_load(&config_);
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
