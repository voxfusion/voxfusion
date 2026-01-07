import TranscriptionList from "../components/TranscriptionList";

export default function Home() {
	return (
		<div class="min-h-screen px-6 py-8">
			<div class="max-w-2xl mx-auto">
				<div class="mb-8">
					<h1 class="text-2xl font-bold text-slate-800">Your Transcriptions</h1>
					<p class="text-slate-500 text-sm mt-1">
						Press Command+; to start a new recording
					</p>
				</div>

				<TranscriptionList />
			</div>
		</div>
	);
}
