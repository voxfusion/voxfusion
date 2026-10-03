/**
 * Types text into an element while keeping its final layout reserved: the
 * untyped remainder stays in the flow, invisible, so lines wrap where they
 * will end up and nothing shifts while the text appears.
 */
export const sleep = (ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms));

export const prefersReducedMotion = () =>
	window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export function renderTyped(el: HTMLElement, text: string, count: number) {
	const rest = document.createElement("span");
	rest.className = "tw-rest";
	rest.textContent = text.slice(count);
	el.replaceChildren(text.slice(0, count), rest);
}

/** Resolves once the text is typed out, or as soon as isCurrent() reports a newer run took over. */
export async function typeText(
	el: HTMLElement,
	text: string,
	msPerChar: number,
	isCurrent: () => boolean = () => true
) {
	const start = performance.now();
	for (let count = 0; count < text.length; ) {
		// Catch up after a throttled timer instead of drifting behind the clock.
		const due = Math.floor((performance.now() - start) / msPerChar);
		count = Math.min(text.length, Math.max(count + 1, due));
		renderTyped(el, text, count);
		await sleep(msPerChar);
		if (!isCurrent()) return;
	}
}
