import { useNavigate } from "@solidjs/router";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Result } from "better-result";
import { type ParentProps, Show, createSignal, onCleanup, onMount } from "solid-js";
import appIcon from "../src-tauri/icons/icon.svg";
import Sidebar from "./components/Navigation";
import SettingsModal from "./components/SettingsModal";
import OnboardingWizard from "./components/onboarding/OnboardingWizard";
import { listInstalledApps } from "./lib/commands/apps";
import { listSiteDictionaries } from "./lib/commands/dictionary";
import { checkModelStatus } from "./lib/commands/model";
import { errorFields, logDiagnostic } from "./lib/diagnostics";
import { preloadFavicons } from "./lib/favicons";
import { MODEL_DOWNLOAD_STEP } from "./lib/onboarding";
import { capture } from "./lib/posthog";
import {
	initSettings,
	markOnboardingComplete,
	resumeOnboardingAt,
	updateMicrophone,
	useSettings,
} from "./lib/settingsStore";

const FORCE_SHOW_ONBOARDING =
	import.meta.env.DEV && import.meta.env.VITE_FORCE_ONBOARDING === "true";

type TauriWindow = Window & {
	__TAURI_INTERNALS__?: unknown;
};

function waitForTauriIPC(): Promise<void> {
	if ((window as TauriWindow).__TAURI_INTERNALS__) return Promise.resolve();

	return new Promise((resolve, reject) => {
		const timeout = setTimeout(() => {
			clearInterval(interval);
			reject(new Error("Tauri IPC bridge not ready after 5s"));
		}, 5000);

		const interval = setInterval(() => {
			if ((window as TauriWindow).__TAURI_INTERNALS__) {
				clearInterval(interval);
				clearTimeout(timeout);
				resolve();
			}
		}, 50);
	});
}

function App(props: ParentProps) {
	const settings = useSettings();
	const navigate = useNavigate();
	const [isSettingsOpen, setIsSettingsOpen] = createSignal(false);
	const [isReady, setIsReady] = createSignal(false);

	const shouldShowOnboarding = () => {
		if (FORCE_SHOW_ONBOARDING) return true;
		return !settings().onboardingComplete;
	};

	onMount(async () => {
		// Cleanups are collected into one synchronously-registered onCleanup:
		// onCleanup calls after an `await` run outside the reactive owner and
		// would silently never fire (see UpdateNotification for the pattern).
		let disposed = false;
		const disposers: (() => void)[] = [];
		onCleanup(() => {
			disposed = true;
			for (const dispose of disposers) dispose();
		});
		const addDisposer = (dispose: () => void) => {
			if (disposed) {
				dispose();
			} else {
				disposers.push(dispose);
			}
		};

		const handleWindowError = (event: ErrorEvent) => {
			logDiagnostic("error", "app", "window_error", {
				message: event.message,
				source: event.filename,
				line: event.lineno,
				column: event.colno,
				error: errorFields(event.error),
			});
		};
		const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
			logDiagnostic("error", "app", "unhandled_rejection", {
				reason: errorFields(event.reason),
			});
		};
		window.addEventListener("error", handleWindowError);
		window.addEventListener("unhandledrejection", handleUnhandledRejection);
		addDisposer(() => {
			window.removeEventListener("error", handleWindowError);
			window.removeEventListener("unhandledrejection", handleUnhandledRejection);
		});

		try {
			logDiagnostic("info", "app", "mount_started");
			capture("app_opened");
			await waitForTauriIPC();

			const appVersion = await Result.tryPromise(() => getVersion());
			logDiagnostic("info", "app", "ipc_ready", {
				version: Result.isOk(appVersion) ? appVersion.value : null,
			});

			await initSettings();
			logDiagnostic("info", "app", "settings_initialized", {
				onboardingComplete: settings().onboardingComplete,
				onboardingStep: settings().onboardingStep,
				defaultStyle: settings().defaultStyle,
			});

			const modelReady = await checkModelStatus();
			if (Result.isOk(modelReady) && !modelReady.value && settings().onboardingComplete) {
				logDiagnostic("warn", "app", "model_missing_resume_onboarding");
				await resumeOnboardingAt(MODEL_DOWNLOAD_STEP);
			}
			if (Result.isError(modelReady)) {
				logDiagnostic("error", "app", "model_status_failed", errorFields(modelReady.error));
				console.error("Failed to verify Whisper model state:", modelReady.error);
			}

			setIsReady(true);
			logDiagnostic("info", "app", "ready");

			void listSiteDictionaries().then((result) => {
				if (Result.isOk(result)) {
					logDiagnostic("debug", "app", "site_dictionaries_loaded", {
						count: result.value.length,
					});
					preloadFavicons(result.value.map((g) => g.domain));
				} else {
					logDiagnostic("error", "app", "site_dictionaries_failed", errorFields(result.error));
				}
			});

			void listInstalledApps().then((result) => {
				if (Result.isOk(result)) {
					logDiagnostic("debug", "app", "installed_apps_loaded", {
						count: result.value.length,
					});
				} else {
					logDiagnostic("error", "app", "installed_apps_failed", errorFields(result.error));
				}
			});

			addDisposer(
				await listen<string>("navigate", (event) => {
					logDiagnostic("debug", "app", "navigate_event", { path: event.payload });
					navigate(event.payload);
				})
			);

			addDisposer(
				await listen<string>("select-microphone", async (event) => {
					const deviceName = event.payload;
					logDiagnostic("debug", "app", "select_microphone_event", {
						hasDeviceName: Boolean(deviceName),
					});
					await updateMicrophone(deviceName || null);
				})
			);

			addDisposer(
				await listen("accessibility-permission-needed", async () => {
					logDiagnostic("warn", "app", "accessibility_permission_needed");
					setIsSettingsOpen(true);
					// The event is emitted from the overlay window while the main
					// window may be hidden in the tray — surface it.
					const mainWindow = getCurrentWindow();
					await Result.tryPromise(async () => {
						await mainWindow.show();
						await mainWindow.setFocus();
					});
				})
			);

			const handleKeyDown = (e: KeyboardEvent) => {
				if (e.metaKey && e.key === ",") {
					e.preventDefault();
					logDiagnostic("debug", "app", "settings_shortcut_pressed");
					setIsSettingsOpen(true);
				}
			};
			window.addEventListener("keydown", handleKeyDown);
			addDisposer(() => window.removeEventListener("keydown", handleKeyDown));
		} catch (error) {
			logDiagnostic("error", "app", "mount_failed", errorFields(error));
			throw error;
		}
	});

	return (
		<div class="relative min-h-screen h-full w-full bg-th-base transition-colors">
			<div
				class="pointer-events-none absolute inset-0 z-0"
				style={{
					"background-image":
						"linear-gradient(var(--color-grid-line) 1px, transparent 1px), linear-gradient(90deg, var(--color-grid-line) 1px, transparent 1px)",
					"background-size": "40px 40px",
				}}
			/>
			<div class="absolute top-0 left-0 right-0 h-6 z-50" data-tauri-drag-region />
			<Show when={!isReady()}>
				<div class="h-full flex flex-col items-center justify-center">
					<img src={appIcon} alt="VoxFusion" class="w-16 h-16 mb-8" />
					<div class="w-48 h-1 bg-border overflow-hidden">
						<div class="w-1/4 h-full bg-ac animate-slide" />
					</div>
				</div>
			</Show>
			<Show when={isReady()}>
				<Show
					when={!shouldShowOnboarding()}
					fallback={
						<OnboardingWizard
							initialStep={settings().onboardingStep}
							onComplete={() => {
								capture("onboarding_completed");
								markOnboardingComplete();
							}}
						/>
					}
				>
					<div class="flex h-full">
						<Sidebar onOpenSettings={() => setIsSettingsOpen(true)} />
						<main class="flex-1 overflow-auto pt-6">{props.children}</main>
					</div>
					<SettingsModal isOpen={isSettingsOpen()} onClose={() => setIsSettingsOpen(false)} />
				</Show>
			</Show>
		</div>
	);
}

export default App;
