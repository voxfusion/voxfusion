import { useI18n } from "../i18n";

export default function About() {
	const [t] = useI18n();

	return (
		<div class="min-h-screen flex items-center justify-center">
			<div class="text-center space-y-8 max-w-2xl px-4">
				<h1 class="text-5xl font-bold bg-gradient-to-r from-purple-400 to-pink-400 bg-clip-text text-transparent">
					{t("about.title")}
				</h1>
				<div class="bg-white/10 backdrop-blur-lg rounded-2xl p-8 space-y-4">
					<p class="text-white text-lg">
						{t("about.welcomeDescription")}
					</p>
					<p class="text-slate-300">
						{t("about.navigationDescription")}
					</p>
				</div>
			</div>
		</div>
	);
}

