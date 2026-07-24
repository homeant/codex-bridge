import "dotenv/config";
import readline from "node:readline";
import AiBot, {
  type EventMessage,
  WSAuthFailureError,
  WSReconnectExhaustedError,
  type ImageMessage,
  type MixedMessage,
  type TextMessage,
  type WsFrame,
} from "@wecom/aibot-node-sdk";
import {
  parseApprovalCardEvent,
  parseImageMessage,
  parseMixedMessage,
  parseTextMessage,
  ReplyManager,
  type BridgeCommand,
  type ParsedMediaMessage,
} from "./adapter.js";
import { InboundImageStore } from "./media.js";

const botId = process.env.WECOM_BOT_ID;
const secret = process.env.WECOM_BOT_SECRET;
if (!botId || !secret) throw new Error("WECOM_BOT_ID and WECOM_BOT_SECRET are required");
const mediaRoot = process.env.IM_CODEX_BRIDGE_MEDIA_ROOT;
if (!mediaRoot) throw new Error("IM_CODEX_BRIDGE_MEDIA_ROOT is required");

const client = new AiBot.WSClient({
  botId,
  secret,
  wsUrl: process.env.WECOM_WS_URL,
  maxReconnectAttempts: -1,
  logger: {
    // SDK debug messages can contain the complete callback frame, including
    // message content and the short-lived response URL. Never forward them.
    debug: () => {},
    info: (message, ...args) => console.error(`[wecom:info] ${message}`, ...args),
    warn: (message, ...args) => console.error(`[wecom:warn] ${message}`, ...args),
    error: (message, ...args) => console.error(`[wecom:error] ${message}`, ...args),
  },
});
const replies = new ReplyManager(client);
const images = new InboundImageStore(client, mediaRoot);

function emit(value: unknown): void {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

client.on("connected", () => console.error("[wecom] websocket connected"));
client.on("authenticated", () => {
  console.error("[wecom] authenticated");
  emit({ type: "ready", platform: "wecom" });
});
client.on("disconnected", (reason) => console.error(`[wecom] disconnected: ${reason}`));
client.on("reconnecting", (attempt) => console.error(`[wecom] reconnecting: ${attempt}`));
client.on("error", (error) => {
  console.error("[wecom] error", error);
  if (error instanceof WSAuthFailureError) {
    emit({
      type: "error",
      platform: "wecom",
      message: "企业微信机器人鉴权失败，请检查 Bot ID 和 Secret。",
    });
  } else if (error instanceof WSReconnectExhaustedError) {
    emit({
      type: "error",
      platform: "wecom",
      message: "企业微信长连接重试次数已耗尽。",
    });
  }
});
client.on("message.text", (frame: WsFrame<TextMessage>) => {
  const parsed = parseTextMessage(frame, botId);
  if (!parsed) return;
  replies.register(parsed.event.message_id, parsed.frame, parsed.chatId);
  emit(parsed.event);
});
client.on("message.image", (frame: WsFrame<ImageMessage>) => {
  const parsed = parseImageMessage(frame, botId);
  if (parsed) handleMediaMessage(parsed);
});
client.on("message.mixed", (frame: WsFrame<MixedMessage>) => {
  const parsed = parseMixedMessage(frame, botId);
  if (parsed) handleMediaMessage(parsed);
});

function handleMediaMessage(parsed: ParsedMediaMessage): void {
  replies.register(parsed.event.message_id, parsed.frame, parsed.chatId);
  if (parsed.images.length === 0) {
    emit(parsed.event);
    return;
  }
  void images
    .saveImages(parsed.event.message_id, parsed.images)
    .then((imagePaths) => emit({ ...parsed.event, image_paths: imagePaths }))
    .catch(async () => {
      // URLs and AES keys are sensitive and short-lived; never include the
      // underlying SDK error in logs or user-visible replies.
      console.error("[wecom] inbound image download or validation failed");
      await replies.handle({
        type: "reply",
        platform: "wecom",
        message_id: parsed.event.message_id,
        chat_id: parsed.chatId,
        content: "图片下载、解密或格式校验失败，请重新发送 PNG、JPEG、GIF 或 WebP 图片。",
        finished: true,
      });
    });
}
client.on("event", (frame: WsFrame<EventMessage>) => {
  const parsed = parseApprovalCardEvent(frame, botId);
  const body = frame.body;
  const event = body?.event as
    | {
        eventtype?: unknown;
        event_key?: unknown;
        task_id?: unknown;
        template_card_event?: { event_key?: unknown; task_id?: unknown };
      }
    | undefined;
  const cardEvent = event?.template_card_event ?? event;
  // Keep this diagnostic deliberately metadata-only. Callback bodies may carry
  // user identifiers and short-lived response data that must never reach logs.
  console.error(
    `[wecom] event callback received event_type=${safeEventType(event?.eventtype)} ` +
      `key_present=${typeof cardEvent?.event_key === "string"} ` +
      `task_present=${typeof cardEvent?.task_id === "string"} ` +
      `bot_match=${body?.aibotid === botId} parsed=${parsed !== undefined}`,
  );
  if (!parsed) return;
  replies.registerApprovalEvent(parsed);
  emit(parsed.event);
});

function safeEventType(value: unknown): string {
  return typeof value === "string" && /^[a-z0-9_]{1,40}$/i.test(value) ? value : "unknown";
}

const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
input.on("line", (line) => {
  try {
    const command = JSON.parse(line) as BridgeCommand;
    if (
      ["reply", "approval", "approval_result"].includes(command.type) &&
      command.platform === "wecom"
    ) {
      void replies.handle(command).catch((error) => {
        console.error("[wecom] reply failed", error);
        emit({ type: "error", platform: "wecom", message: String(error) });
      });
    }
  } catch (error) {
    console.error("[wecom] invalid bridge command", error);
  }
});
input.on("close", () => {
  client.disconnect();
  process.exitCode = 0;
});

client.connect();
