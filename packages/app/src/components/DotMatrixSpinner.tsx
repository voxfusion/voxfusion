import { For } from "solid-js";

const MATRIX_SIZE = 5;
const CENTER = Math.floor(MATRIX_SIZE / 2);

const DOTS = Array.from({ length: MATRIX_SIZE * MATRIX_SIZE }, (_, i) => {
	const row = Math.floor(i / MATRIX_SIZE);
	const col = i % MATRIX_SIZE;
	const manhattan = Math.abs(row - CENTER) + Math.abs(col - CENTER);
	const ring = Math.max(0, Math.min(4, manhattan));
	return { row, col, ring, parity: ring % 2 };
});

type DotMatrixSpinnerProps = {
	size?: number;
	dotSize?: number;
	cycleMs?: number;
	class?: string;
};

export default function DotMatrixSpinner(props: DotMatrixSpinnerProps) {
	const size = () => props.size ?? 14;
	const dotSize = () => props.dotSize ?? 2;
	const cycleMs = () => props.cycleMs ?? 1500;
	const gap = () => (size() - dotSize() * MATRIX_SIZE) / (MATRIX_SIZE - 1);

	return (
		<div
			role="status"
			aria-label="Loading"
			class={props.class}
			style={{
				display: "grid",
				"grid-template-columns": `repeat(${MATRIX_SIZE}, ${dotSize()}px)`,
				"grid-template-rows": `repeat(${MATRIX_SIZE}, ${dotSize()}px)`,
				gap: `${gap()}px`,
				width: `${size()}px`,
				height: `${size()}px`,
			}}
		>
			<For each={DOTS}>
				{(dot) => (
					<div
						style={{
							"border-radius": "9999px",
							"background-color": "currentColor",
							"animation-name": "dmx-ripple-echo",
							"animation-duration": `${cycleMs()}ms`,
							"animation-timing-function": "ease-in-out",
							"animation-iteration-count": "infinite",
							"animation-delay": `${(dot.ring * 0.14 + dot.parity * 0.03) * cycleMs()}ms`,
						}}
					/>
				)}
			</For>
		</div>
	);
}
