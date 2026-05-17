import type { CommandResult } from "./invokeResult";
import { invokeResult } from "./invokeResult";

export async function checkAccessibilityProbe(): Promise<CommandResult<boolean>> {
	return invokeResult<boolean>("check_accessibility_probe");
}
