import { useI18n } from "../i18n";
import { BookOpen } from "lucide-solid";

export default function Dictionary() {
	const [t] = useI18n();

	return (
		<div class="min-h-screen px-6 py-8">
			<div class="max-w-2xl mx-auto">
				<div class="mb-8">
					<h1 class="text-2xl font-bold text-slate-800">{t("dictionary.title")}</h1>
					<p class="text-slate-500 text-sm mt-1">{t("dictionary.description")}</p>
				</div>

				<div class="bg-white rounded-xl p-12 shadow-sm border border-slate-200 flex flex-col items-center justify-center text-center">
					<div class="w-16 h-16 bg-slate-100 rounded-full flex items-center justify-center mb-4">
						<BookOpen class="w-8 h-8 text-slate-400" />
					</div>
					<h2 class="text-lg font-semibold text-slate-700 mb-2">
						{t("dictionary.comingSoon")}
					</h2>
					<p class="text-slate-500 text-sm max-w-sm">
						{t("dictionary.comingSoonDescription")}
					</p>
				</div>
			</div>
		</div>
	);
}
