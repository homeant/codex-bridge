import "dotenv/config";
import readline from "node:readline";
import { LoggerLevel, createLarkChannel } from "@larksuiteoapi/node-sdk";

type ReplyCommand = {
  type: "reply";
  platform: "lark";
  message_id: string;
  chat_id: string;
  thread_id?: string;
  content: string;
  finished: boolean;
};

class AsyncTextQueue implements AsyncIterable<string> {
  private values: string[] = [];
  private waiters: Array<(value: IteratorResult<string>) => void> = [];
  private closed = false;

  push(value: string): void {
    if (this.closed || !value) return;
    const waiter = this.waiters.shift();
    if (waiter) waiter({ value, done: false });
    else this.values.push(value);
  }

  close(): void {
    this.closed = true;
    for (const waiter of this.waiters.splice(0)) waiter({ value: undefined, done: true });
  }

  [Symbol.asyncIterator](): AsyncIterator<string> {
    return {
      next: async () => {
        const value = this.values.shift();
        if (value !== undefined) return { value, done: false };
        if (this.closed) return { value: undefined, done: true };
        return new Promise<IteratorResult<string>>((resolve) => this.waiters.push(resolve));
      },
    };
  }
}

type ReplyContext = { queue: AsyncTextQueue; lastContent: string };

const appId = process.env.FEISHU_APP_ID;
const appSecret = process.env.FEISHU_APP_SECRET;
if (!appId || !appSecret) throw new Error("FEISHU_APP_ID and FEISHU_APP_SECRET are required");

const channel = createLarkChannel({
  appId,
  appSecret,
  loggerLevel: LoggerLevel.info,
  logger: {
    trace: (...args) => console.error("[lark:trace]", ...args),
    debug: (...args) => console.error("[lark:debug]", ...args),
    info: (...args) => console.error("[lark:info]", ...args),
    warn: (...args) => console.error("[lark:warn]", ...args),
    error: (...args) => console.error("[lark:error]", ...args),
  },
  policy: { requireMention: true, dmMode: "open" },
});
const contexts = new Map<string, ReplyContext>();

function emit(value: unknown): void {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

channel.on("message", (message) => {
  const queue = new AsyncTextQueue();
  const threadId = message.threadId;
  contexts.set(message.messageId, { queue, lastContent: "" });
  emit({
    type: "message",
    platform: "lark",
    message_id: message.messageId,
    chat_id: message.chatId,
    chat_type: message.chatType === "group" ? "group" : "direct",
    user_id: message.senderId,
    content: message.content,
    mentioned_bot: message.chatType === "p2p" || message.mentionedBot,
    thread_id: threadId,
    root_id: message.rootId,
  });

  void channel
    .stream(
      message.chatId,
      {
        markdown: async (stream) => {
          for await (const chunk of queue) await stream.append(chunk);
        },
      },
      { replyTo: message.messageId, replyInThread: threadId !== undefined },
    )
    .catch((error) => {
      console.error("[lark] stream failed", error);
      emit({ type: "error", platform: "lark", message: String(error) });
    })
    .finally(() => contexts.delete(message.messageId));
});
channel.on("reject", (event) => console.error("[lark] message rejected", event));
channel.on("error", (error) => console.error("[lark] inbound error", error));
channel.on("reconnecting", () => console.error("[lark] reconnecting"));
channel.on("reconnected", () => console.error("[lark] reconnected"));

function handleReply(command: ReplyCommand): void {
  const context = contexts.get(command.message_id);
  if (!context) {
    void channel
      .send(
        command.chat_id,
        { markdown: command.content },
        { replyTo: command.message_id, replyInThread: command.thread_id !== undefined },
      )
      .catch((error) => console.error("[lark] fallback send failed", error));
    return;
  }
  const delta = command.content.startsWith(context.lastContent)
    ? command.content.slice(context.lastContent.length)
    : `\n\n${command.content}`;
  context.lastContent = command.content;
  context.queue.push(delta);
  if (command.finished) context.queue.close();
}

const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
input.on("line", (line) => {
  try {
    const command = JSON.parse(line) as ReplyCommand;
    if (command.type === "reply" && command.platform === "lark") handleReply(command);
  } catch (error) {
    console.error("[lark] invalid bridge command", error);
  }
});
input.on("close", () => {
  void channel.disconnect().finally(() => { process.exitCode = 0; });
});

await channel.connect();
console.error(`[lark] connected as ${channel.botIdentity?.name ?? "bot"}`);
emit({ type: "ready", platform: "lark" });
