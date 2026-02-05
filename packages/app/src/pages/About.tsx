import { useI18n } from "../i18n";

export default function About() {
	const [t] = useI18n();

	return (
		<div class="min-h-screen flex items-center justify-center">
			<div class="text-center space-y-8 max-w-2xl px-4">
				<div class="font-mono text-[#888] text-sm tracking-wider mb-2">
					[ABOUT] &gt; VOXFUSION
				</div>
				<h1 class="text-5xl font-bold text-[#ff3e00] font-mono uppercase tracking-wider">
					{t("about.title")}
				</h1>
				<div class="bg-[#111] border border-[#222] p-8 space-y-4">
					<p class="text-[#e0e0e0] text-lg font-mono">
						{t("about.welcomeDescription")}
					</p>
					<p class="text-[#888] font-mono">
						{t("about.navigationDescription")}
					</p>
				</div>
			</div>
		</div>
	);
}

