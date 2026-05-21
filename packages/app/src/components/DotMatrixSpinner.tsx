import { For } from "solid-js";

const MATRIX_SIZE = 5;
const CENTER = Math.floor(MATRIX_SIZE / 2);
const RING_PATH: readonly number[] = [1, 2, 3, 9, 14, 19, 23, 22, 21, 15, 10, 5];

const DOTS = Array.from({ length: MATRIX_SIZE * MATRIX_SIZE }, (_, index) => {
	const row = Math.floor(index / MATRIX_SIZE);
	const col = index % MATRIX_SIZE;
	const distance = Math.hypot(row - CENTER, col - CENTER);
	const ringOrder = RING_PATH.indexOf(index);

	return {
		isCenter: row === CENTER && col === CENTER,
		isVisible: distance <= CENTER,
		ringOrder,
	};
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
	const cycleMs = () => props.cycleMs ?? 1200;
	const gap = () => (size() - dotSize() * MATRIX_SIZE) / (MATRIX_SIZE - 1);

	return (
		<output
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
							opacity: dot.isVisible ? (dot.isCenter ? 0.18 : 0.08) : 0,
							...(dot.ringOrder >= 0
								? {
										"animation-name": "dotm-circular-2",
										"animation-duration": `${cycleMs()}ms`,
										"animation-timing-function": "steps(12, end)",
										"animation-iteration-count": "infinite",
										"animation-delay": `${(dot.ringOrder / RING_PATH.length) * cycleMs()}ms`,
									}
								: {}),
						}}
					/>
				)}
			</For>
		</output>
	);
}
