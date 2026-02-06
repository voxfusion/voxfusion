import { useI18n } from "../i18n";

export default function About() {
	const [t] = useI18n();

	return (
		<div class="min-h-screen flex items-center justify-center">
			<div class="text-center space-y-8 max-w-2xl px-4">
				<div class="font-mono text-txt-secondary text-sm tracking-wider mb-2">
					[ABOUT] &gt; VOXFUSION
				</div>
				<h1 class="text-5xl font-bold text-ac font-mono uppercase tracking-wider">
					{t("about.title")}
				</h1>
				<div class="bg-th-surface border border-border p-8 space-y-4">
					<p class="text-txt-primary text-lg font-mono">{t("about.welcomeDescription")}</p>
					<p class="text-txt-secondary font-mono">{t("about.navigationDescription")}</p>
				</div>
			</div>
		</div>
	);
}
