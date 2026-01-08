import { createSignal } from "solid-js";
import { useI18n, type Locale } from "../i18n";

export default function Settings() {
	const [t, { locale, setLocale }] = useI18n();
	const [theme, setTheme] = createSignal("dark");

	return (
		<div class="min-h-screen flex items-center justify-center">
			<div class="text-center space-y-8 max-w-2xl px-4">
				<h1 class="text-5xl font-bold bg-gradient-to-r from-purple-400 to-pink-400 bg-clip-text text-transparent">
					{t("settings.title")}
				</h1>
				<div class="bg-white/10 backdrop-blur-lg rounded-2xl p-8 space-y-6">
					<div class="space-y-4">
						<label for="theme-select" class="block text-left text-white font-medium">
							{t("settings.theme")}
						</label>
						<select
							id="theme-select"
							value={theme()}
							onChange={(e) => setTheme(e.currentTarget.value)}
							class="w-full px-4 py-2 bg-white/10 border border-white/20 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-purple-500"
						>
							<option value="dark">{t("settings.dark")}</option>
							<option value="light">{t("settings.light")}</option>
							<option value="auto">{t("settings.auto")}</option>
						</select>
					</div>
					<p class="text-slate-300 text-sm">
						{t("settings.currentTheme")} <span class="text-white font-semibold">{theme()}</span>
					</p>

					<div class="space-y-4 pt-4 border-t border-white/10">
						<label for="language-select" class="block text-left text-white font-medium">
							{t("settings.language")}
						</label>
						<select
							id="language-select"
							value={locale()}
							onChange={(e) => setLocale(e.currentTarget.value as Locale)}
							class="w-full px-4 py-2 bg-white/10 border border-white/20 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-purple-500"
						>
							<option value="en">{t("settings.english")}</option>
							<option value="ru">{t("settings.russian")}</option>
						</select>
					</div>
				</div>
			</div>
		</div>
	);
}
