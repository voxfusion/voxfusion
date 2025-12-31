import { treaty } from "@elysiajs/eden";
import type { App } from "../../../server/src";

const client = treaty<App>('localhost:3000')

export default client;