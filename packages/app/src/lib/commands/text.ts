import type { CommandResult } from "./invokeResult";
import { invokeResult } from "./invokeResult";

export async function typeText(text: string): Promise<CommandResult<void>> {
	return invokeResult<void>("type_text", { text });
}
