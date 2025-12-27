import { createSignal } from "solid-js";

function App() {
	const [count, setCount] = createSignal(0);

	return (
		<div class="min-h-screen bg-gradient-to-br from-slate-900 via-purple-900 to-slate-900 flex items-center justify-center">
			<div class="text-center space-y-8">
				<h1 class="text-5xl font-bold bg-gradient-to-r from-purple-400 to-pink-400 bg-clip-text text-transparent">
					VoxFusion
				</h1>
				<p class="text-slate-300 text-lg">Desktop Application</p>

				<div class="bg-white/10 backdrop-blur-lg rounded-2xl p-8 space-y-6">
					<p class="text-white text-2xl font-semibold">Count: {count()}</p>
					<div class="flex gap-4 justify-center">
						<button
							type="button"
							onClick={() => setCount((c) => c - 1)}
							class="px-6 py-3 bg-pink-500 hover:bg-pink-600 text-white font-medium rounded-xl transition-colors"
						>
							Decrement
						</button>
						<button
							type="button"
							onClick={() => setCount((c) => c + 1)}
							class="px-6 py-3 bg-purple-500 hover:bg-purple-600 text-white font-medium rounded-xl transition-colors"
						>
							Increment
						</button>
					</div>
				</div>
			</div>
		</div>
	);
}

export default App;
