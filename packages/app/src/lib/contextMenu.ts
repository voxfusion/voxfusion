export function disableContextMenu(): void {
	if (import.meta.env.DEV) return;

	window.addEventListener("contextmenu", (event) => {
		event.preventDefault();
	});
}
