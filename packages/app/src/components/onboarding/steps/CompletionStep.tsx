import { CheckCircle } from "lucide-solid";
import { useI18n } from "../../../i18n";

export default function CompletionStep() {
	const [t] = useI18n();

	return (
		<div class="text-center max-w-md mx-auto">
			{/* Terminal-style header */}
			<div class="font-mono text-[#00ff88] text-sm mb-8 tracking-wider">
				[STEP_06] &gt; SETUP_COMPLETE
			</div>

			{/* Card container */}
			<div class="border border-[#222] bg-[#111] p-8">
				<div class="w-20 h-20 border border-[#00ff88] flex items-center justify-center mx-auto mb-6">
					<CheckCircle class="w-12 h-12 text-[#00ff88]" />
				</div>

				<h2 class="font-mono text-2xl uppercase tracking-wider text-[#e0e0e0] mb-3">
					{t("onboarding.completionTitle")}
				</h2>

				<p class="font-mono text-sm text-[#888]">
					{t("onboarding.completionDescription")}
				</p>

				{/* Decorative element */}
				<div class="mt-8 pt-6 border-t border-[#222]">
					<div class="font-mono text-xs text-[#666] tracking-wider">
						&gt; READY_TO_TRANSCRIBE
					</div>
				</div>
			</div>
		</div>
	);
}
