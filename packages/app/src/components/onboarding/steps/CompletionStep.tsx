import { CheckCircle } from "lucide-solid";
import { useI18n } from "../../../i18n";

export default function CompletionStep() {
	const [t] = useI18n();

	return (
		<div class="text-center max-w-md mx-auto">
			<div class="w-24 h-24 bg-green-100 dark:bg-green-900/30 rounded-full flex items-center justify-center mx-auto mb-6">
				<CheckCircle class="w-14 h-14 text-green-600 dark:text-green-400" />
			</div>

			<h2 class="text-3xl font-bold text-slate-900 dark:text-white mb-3">
				{t("onboarding.completionTitle")}
			</h2>

			<p class="text-lg text-slate-600 dark:text-slate-400">
				{t("onboarding.completionDescription")}
			</p>
		</div>
	);
}
