import TranscriptionList from "../components/TranscriptionList";
import { useI18n } from "../i18n";

export default function Home() {
	const [t] = useI18n();

	return (
		<div class="min-h-screen px-6 py-8">
			<div class="max-w-2xl mx-auto">
				<div class="mb-8">
					<h1 class="text-2xl font-bold text-slate-800 dark:text-white">{t("home.yourTranscriptions")}</h1>
					<p class="text-slate-500 dark:text-slate-400 text-sm mt-1">
						{t("home.pressToRecord")}
					</p>
				</div>

				<TranscriptionList />
			</div>
		</div>
	);
}
