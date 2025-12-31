export default function About() {
	return (
		<div class="min-h-screen flex items-center justify-center">
			<div class="text-center space-y-8 max-w-2xl px-4">
				<h1 class="text-5xl font-bold bg-gradient-to-r from-purple-400 to-pink-400 bg-clip-text text-transparent">
					About
				</h1>
				<div class="bg-white/10 backdrop-blur-lg rounded-2xl p-8 space-y-4">
					<p class="text-white text-lg">
						Welcome to VoxFusion - a modern desktop application built with
						SolidJS and Tauri.
					</p>
					<p class="text-slate-300">
						This application demonstrates page navigation and routing
						functionality.
					</p>
				</div>
			</div>
		</div>
	);
}

