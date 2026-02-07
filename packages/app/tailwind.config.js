export default {
	darkMode: "class",
	content: ["./index.html", "./voice-control.html", "./src/**/*.{js,ts,jsx,tsx}"],
	theme: {
		extend: {
			colors: {
				// Semantic theme-aware colors via CSS custom properties
				th: {
					base: "var(--color-bg-base)",
					surface: "var(--color-bg-surface)",
					elevated: "var(--color-bg-elevated)",
					hover: "var(--color-bg-hover)",
					input: "var(--color-bg-input)",
					overlay: "var(--color-bg-overlay)",
				},
				border: {
					DEFAULT: "var(--color-border)",
					strong: "var(--color-border-strong)",
					subtle: "var(--color-border-subtle)",
				},
				txt: {
					primary: "var(--color-text-primary)",
					secondary: "var(--color-text-secondary)",
					muted: "var(--color-text-muted)",
					faint: "var(--color-text-faint)",
				},
				ac: {
					DEFAULT: "var(--color-accent)",
					hover: "var(--color-accent-hover)",
					bg: "var(--color-accent-bg)",
					on: "var(--color-accent-on)",
				},
				success: "var(--color-success)",
			},
			fontFamily: {
				sans: [
					"Inter",
					"system-ui",
					"-apple-system",
					"BlinkMacSystemFont",
					"Segoe UI",
					"Roboto",
					"sans-serif",
				],
			},
		},
	},
	plugins: [],
};
