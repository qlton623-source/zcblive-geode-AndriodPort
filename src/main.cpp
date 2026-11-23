#include <Geode/Geode.hpp>
#include <Geode/binding/FMODAudioEngine.hpp>
#include <Geode/cocos/shaders/ccGLStateCache.h>
#include <Geode/modify/CCDirector.hpp>
#include <Geode/modify/CCIMEDispatcher.hpp>
#include <Geode/modify/CCKeyboardDispatcher.hpp>
#include <Geode/modify/CCTouchDispatcher.hpp>
#include <Geode/modify/GJBaseGameLayer.hpp>
#include <Geode/modify/LevelEditorLayer.hpp>
#include <Geode/modify/PlayLayer.hpp>
#include <Geode/modify/PlayerObject.hpp>
#include <Geode/modify/System.hpp>
#include <Geode/utils/AndroidEvent.hpp>
#include <Geode/utils/file.hpp>
#include <Geode/utils/general.hpp>
#include <Geode/utils/web.hpp>

#include <cstdlib>
#include <string>
#include <variant>
#include <vector>

using namespace geode::prelude;

// Rust library FFI (libzcblive.a statically linked into the mod binary)
extern "C" {
typedef void (*ZcbStrCallback)(const char*);
typedef void (*ZcbBoolCallback)(bool);

void zcblive_set_save_dir(const char* dir);
void zcblive_set_open_url_cb(ZcbStrCallback cb);
void zcblive_set_clipboard_cb(ZcbStrCallback cb);
void zcblive_set_open_folder_cb(ZcbStrCallback cb);
void zcblive_set_ime_cb(ZcbBoolCallback cb);
void zcblive_on_fmod_ready(void* fmod_system);

void zcblive_on_swap_buffers(uint32_t width, uint32_t height);
bool zcblive_on_touch(uint8_t action, int32_t pointer_id, float x, float y);
bool zcblive_on_key(int32_t cocos_key, bool pressed, bool repeat);
bool zcblive_on_text(const char* text);
bool zcblive_on_scroll(float dx, float dy);

void zcblive_initialize();
void zcblive_uninitialize();
void zcblive_on_action(uint8_t button, bool player2, bool push);
void zcblive_on_reset();
void zcblive_set_is_in_level(bool is_in_level);
void zcblive_set_playlayer_time(double time);
void zcblive_on_init(PlayLayer* playlayer);
void zcblive_on_quit();
void zcblive_on_death();
bool zcblive_do_force_player2_sounds();
bool zcblive_do_use_alternate_hook();
void zcblive_on_update(float dt);
}

namespace {
bool g_fmodReady = false;

void onOpenUrl(const char* url) {
    if (url && *url) {
        geode::utils::web::openLinkInBrowser(url);
    }
}

void onClipboardWrite(const char* text) {
    if (text) {
        geode::utils::clipboard::write(text);
    }
}

void onOpenFolder(const char* path) {
    if (path && *path) {
        geode::utils::file::openFolder(std::filesystem::path(path));
    }
}

void onImeToggle(bool open) {
    if (auto* view = CCEGLView::get()) {
        view->setIMEKeyboardState(open);
    }
}
} // namespace

// Capture Geometry Dash's own FMOD::System as soon as it finishes initializing.
// clang-format off
class $modify(ZcbFMODSystem, FMOD::System) {
    FMOD_RESULT init(int maxChannels, FMOD_INITFLAGS flags, void* extraData) {
        FMOD_RESULT res = FMOD::System::init(maxChannels, flags, extraData);
        if (res == FMOD_OK) {
            g_fmodReady = true;
            zcblive_on_fmod_ready(static_cast<void*>(this));
        }
        return res;
    }
};

// On Android, CCEGLView::swapBuffers() is managed by GLSurfaceView in Java,
// so hook CCDirector::drawScene() to render the egui overlay right after the
// scene finishes drawing and before GLSurfaceView swaps buffers.
class $modify(ZcbDirector, CCDirector) {
    void drawScene() {
        CCDirector::drawScene();

        if (!g_fmodReady) {
            if (auto* engine = FMODAudioEngine::get()) {
                if (engine->m_system) {
                    g_fmodReady = true;
                    zcblive_on_fmod_ready(static_cast<void*>(engine->m_system));
                }
            }
        }

        if (auto* view = CCEGLView::get()) {
            auto frameSize = view->getFrameSize();
            auto w = static_cast<uint32_t>(frameSize.width > 1.0f ? frameSize.width : 1.0f);
            auto h = static_cast<uint32_t>(frameSize.height > 1.0f ? frameSize.height : 1.0f);
            zcblive_on_swap_buffers(w, h);
            cocos2d::ccGLInvalidateStateCache();
        }
    }
};

class $modify(ZcbTouchDispatcher, CCTouchDispatcher) {
    void touches(CCSet* pTouches, CCEvent* pEvent, unsigned int uIndex) {
        if (pTouches && pTouches->count() > 0) {
            bool anyConsumed = false;
            std::vector<CCObject*> unconsumed;
            unconsumed.reserve(static_cast<size_t>(pTouches->count()));

            for (auto it = pTouches->begin(); it != pTouches->end(); ++it) {
                bool consumed = false;
                if (auto* touch = typeinfo_cast<CCTouch*>(*it)) {
                    auto loc = touch->getLocationInView();
                    consumed = zcblive_on_touch(
                        static_cast<uint8_t>(uIndex),
                        static_cast<int32_t>(touch->getID()),
                        loc.x,
                        loc.y);
                }
                if (consumed) {
                    anyConsumed = true;
                } else {
                    unconsumed.push_back(*it);
                }
            }

            if (anyConsumed) {
                if (unconsumed.empty()) {
                    return;
                }
                CCSet remaining;
                for (auto* obj : unconsumed) {
                    remaining.addObject(obj);
                }
                CCTouchDispatcher::touches(&remaining, pEvent, uIndex);
                return;
            }
        }
        CCTouchDispatcher::touches(pTouches, pEvent, uIndex);
    }
};

class $modify(ZcbKeyboardDispatcher, CCKeyboardDispatcher) {
    bool dispatchKeyboardMSG(enumKeyCodes key, bool isKeyDown, bool isKeyRepeat, double timestamp) {
        if (zcblive_on_key(static_cast<int32_t>(key), isKeyDown, isKeyRepeat)) {
            return true;
        }
        return CCKeyboardDispatcher::dispatchKeyboardMSG(key, isKeyDown, isKeyRepeat, timestamp);
    }
};

class $modify(ZcbIMEDispatcher, CCIMEDispatcher) {
    void dispatchInsertText(const char* text, int len, enumKeyCodes key) {
        if (text && len > 0) {
            std::string utf8(text, static_cast<size_t>(len));
            if (zcblive_on_text(utf8.c_str())) {
                return;
            }
        }
        CCIMEDispatcher::dispatchInsertText(text, len, key);
    }

    void dispatchDeleteBackward() {
        if (zcblive_on_key(8 /* Backspace */, true, false)) {
            zcblive_on_key(8, false, false);
            return;
        }
        CCIMEDispatcher::dispatchDeleteBackward();
    }
};
// clang-format on

void onUnload() {
    zcblive_uninitialize();
}

$on_mod(Loaded) {
    // Must be set before BOT Lazy initializes Config/Env
    auto saveDir = Mod::get()->getSaveDir().string();
    zcblive_set_save_dir(saveDir.c_str());

    zcblive_set_open_url_cb(onOpenUrl);
    zcblive_set_clipboard_cb(onClipboardWrite);
    zcblive_set_open_folder_cb(onOpenFolder);
    zcblive_set_ime_cb(onImeToggle);

    AndroidRichInputEvent().listen([](int64_t, int, int, AndroidRichInput const& input) {
        if (auto* scroll = std::get_if<AndroidScrollInput>(&input)) {
            if (zcblive_on_scroll(scroll->scrollX(), scroll->scrollY())) {
                return ListenerResult::Stop;
            }
        }

        // AndroidRichInputEvent is sent before Geode forwards the MotionEvent
        // into Cocos. Handle the egui overlay here, so UI input does not depend
        // on hooking CCTouchDispatcher's internal dispatch method.
        if (auto* touch = std::get_if<AndroidTouchInput>(&input)) {
            auto const& points = touch->touches();
            if (!points.empty()) {
                // Geode 5.11.2's Android input implementation currently
                // pre-sizes this vector and then emplaces the same points,
                // leaving a zero-initialized prefix followed by real points.
                // Use the real second half for this SDK version.
                auto firstActualPoint = points.size();
                if (points.size() % 2 == 0) {
                    firstActualPoint = points.size() / 2;
                }

                auto action = static_cast<uint8_t>(touch->type());
                bool consumed = false;
                for (auto i = firstActualPoint; i < points.size(); ++i) {
                    auto const& point = points[i];
                    if (zcblive_on_touch(action, point.id, point.x, point.y)) {
                        consumed = true;
                    }
                }

                if (consumed) {
                    return ListenerResult::Stop;
                }
            }
        }

        return ListenerResult::Propagate;
    }).leak();

    zcblive_initialize();
    std::atexit(onUnload);
}

inline double getTime() {
    auto playLayer = PlayLayer::get();
    return playLayer ? playLayer->m_currentTime : 0.0;
}

void handleAction(int button, bool player1, bool push, PlayLayer* playLayer) {
    bool twoPlayer = playLayer && playLayer->m_levelSettings &&
                     playLayer->m_levelSettings->m_twoPlayerMode;
    zcblive_on_action(
        static_cast<uint8_t>(button),
        !player1 && playLayer && (twoPlayer || zcblive_do_force_player2_sounds()),
        push);
}

// clang-format off
class $modify(PlayerObject) {
	void handlePushOrRelease(PlayerButton button, bool push) {
		auto playLayer = PlayLayer::get();
		if (playLayer == nullptr && LevelEditorLayer::get() == nullptr) {
			zcblive_set_is_in_level(false);
			return;
		}
		if ((button == PlayerButton::Left || button == PlayerButton::Right) && !this->m_isPlatformer) {
			return;
		}

		zcblive_set_is_in_level(true);
		zcblive_set_playlayer_time(getTime());

		bool player1 = playLayer && this == playLayer->m_player1;
		handleAction(static_cast<int>(button), player1, push, playLayer);
	}

	bool pushButton(PlayerButton button) {
		if (zcblive_do_use_alternate_hook()) {
			handlePushOrRelease(button, true);
		}
		return PlayerObject::pushButton(button);
	}

	bool releaseButton(PlayerButton button) {
		if (zcblive_do_use_alternate_hook()) {
			handlePushOrRelease(button, false);
		}
		return PlayerObject::releaseButton(button);
	}
};

class $modify(GJBaseGameLayer) {
	void processCommands(float dt, bool isHalfTick, bool isLastTick) {
		bool lastDied = m_playerDied;
		m_playerDied = false;
		GJBaseGameLayer::processCommands(dt, isHalfTick, isLastTick);
		m_playerDied = lastDied;
	}

	void handleButton(bool push, int button, bool player1) {
		if (zcblive_do_use_alternate_hook()) {
			GJBaseGameLayer::handleButton(push, button, player1);
			return;
		}
		zcblive_set_is_in_level(true);
		zcblive_set_playlayer_time(getTime());

		auto playLayer = PlayLayer::get();
		bool p1Plat = playLayer && playLayer->m_player1 && playLayer->m_player1->m_isPlatformer;
		bool p2Plat = playLayer && playLayer->m_player2 && playLayer->m_player2->m_isPlatformer;
		bool is_invalid = playLayer && ((button == 2 || button == 3)
				&& !(player1 && p1Plat)
				&& !(!player1 && p2Plat));
		if (!is_invalid) {
			handleAction(button, player1, push, playLayer);
		}

		GJBaseGameLayer::handleButton(push, button, player1);
	}

	void update(float dt) {
		zcblive_on_update(dt);
		GJBaseGameLayer::update(dt);
		zcblive_set_playlayer_time(getTime());
	}

	bool init() {
		zcblive_on_init(nullptr); // PlayLayer* could be nullptr on Geode
		return GJBaseGameLayer::init();
	}
};

class $modify(PlayLayer) {
	void onQuit() {
		zcblive_on_quit();
		PlayLayer::onQuit();
	}

	void resetLevel() {
		zcblive_on_reset();
		PlayLayer::resetLevel();
	}

	void destroyPlayer(PlayerObject* player, GameObject* hit) {
		PlayLayer::destroyPlayer(player, hit);
		if (player && player->m_isDead) {
			zcblive_on_death();
		}
	}
};

class $modify(LevelEditorLayer) {
	bool init(GJGameLevel* level, bool something) {
		zcblive_on_init(nullptr);
		return LevelEditorLayer::init(level, something);
	}
};
// clang-format on
