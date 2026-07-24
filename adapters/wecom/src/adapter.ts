import {
  EventType,
  generateReqId,
  type BaseMessage,
  type EventMessage,
  type ImageMessage,
  type MixedMessage,
  type SendMsgBody,
  type TemplateCard,
  type TemplateCardEventData,
  type TextMessage,
  type WsFrame,
} from "@wecom/aibot-node-sdk";
import type { InboundImageSource } from "./media.js";

export type ReplyCommand = {
  type: "reply";
  platform: "wecom";
  message_id: string;
  chat_id: string;
  content: string;
  finished: boolean;
};

export type ApprovalCommand = {
  type: "approval";
  platform: "wecom";
  message_id: string;
  chat_id: string;
  token: string;
  task_id: string;
  title: string;
  content: string;
};

export type ApprovalResultCommand = {
  type: "approval_result";
  platform: "wecom";
  message_id: string;
  chat_id: string;
  task_id: string;
  status: "approved" | "denied" | "error";
  content: string;
};

export type BridgeCommand = ReplyCommand | ApprovalCommand | ApprovalResultCommand;

export type MessageEvent = {
  type: "message";
  platform: "wecom";
  message_id: string;
  chat_id: string;
  chat_type: "group" | "direct";
  user_id: string;
  content: string;
  mentioned_bot: true;
  card_task_id?: string;
  image_paths?: string[];
  quoted_text?: string;
};

export type ParsedTextMessage = {
  event: MessageEvent;
  frame: WsFrame<TextMessage>;
  chatId: string;
};

export type ParsedMediaMessage = {
  event: MessageEvent;
  frame: Pick<WsFrame, "headers">;
  chatId: string;
  images: InboundImageSource[];
};

type ApprovalEventFrame = WsFrame<EventMessage>;

export type ParsedApprovalCardEvent = {
  event: MessageEvent;
  frame: ApprovalEventFrame;
  chatId: string;
  taskId: string;
  userId: string;
};

export interface ReplyClient {
  replyStreamNonBlocking(
    frame: Pick<WsFrame, "headers">,
    streamId: string,
    content: string,
    finish?: boolean,
  ): Promise<WsFrame | "skipped">;
  sendMessage(chatId: string, body: SendMsgBody): Promise<WsFrame>;
  updateTemplateCard(
    frame: Pick<WsFrame, "headers">,
    templateCard: TemplateCard,
    userIds?: string[],
  ): Promise<WsFrame>;
}

type ReplyContext = {
  frame: Pick<WsFrame, "headers">;
  chatId: string;
  streamId: string;
  expired: boolean;
};

type ApprovalEventContext = {
  frame: ApprovalEventFrame;
  taskId: string;
  userId: string;
};

const STREAM_EXPIRED_ERROR_CODE = 846608;
const MAX_STREAM_CONTENT_BYTES = 20_480;

/**
 * WeCom's AI Bot callback protocol only delivers an internal group message to
 * the bot when the bot is mentioned. Unlike Lark, the callback has no mention
 * list to inspect, so a valid group callback is the platform-level mention
 * signal. Checking aibotid prevents a malformed/cross-wired frame from being
 * attributed to this configured bot.
 */
export function parseTextMessage(
  frame: WsFrame<TextMessage>,
  expectedBotId: string,
): ParsedTextMessage | undefined {
  const body = frame.body;
  if (!body || body.msgtype !== "text" || body.aibotid !== expectedBotId) return undefined;
  if (!frame.headers?.req_id) return undefined;

  const messageId = body.msgid;
  const userId = body.from?.userid;
  const chatId = body.chattype === "group" ? body.chatid : userId;
  const content = body.text?.content;
  if (!messageId || !userId || !chatId || !content?.trim()) return undefined;

  return {
    frame,
    chatId,
    event: {
      type: "message",
      platform: "wecom",
      message_id: messageId,
      chat_id: chatId,
      chat_type: body.chattype === "group" ? "group" : "direct",
      user_id: userId,
      // Preserve the original request. Rust performs its own empty check.
      content,
      mentioned_bot: true,
      // Keep an empty string when a non-text message was quoted so Rust can
      // distinguish "quoted but not routable" from "no quote, start new".
      quoted_text: body.quote === undefined ? undefined : (extractQuotedText(body.quote) ?? ""),
    },
  };
}

export function parseImageMessage(
  frame: WsFrame<ImageMessage>,
  expectedBotId: string,
): ParsedMediaMessage | undefined {
  const body = frame.body;
  if (!body || body.msgtype !== "image" || !body.image?.url) return undefined;
  const parsed = parseMessageEnvelope(frame, body, expectedBotId);
  if (!parsed) return undefined;
  return {
    ...parsed,
    event: { ...parsed.event, content: "请分析我发送的图片。" },
    images: [{ url: body.image.url, aesKey: body.image.aeskey }],
  };
}

export function parseMixedMessage(
  frame: WsFrame<MixedMessage>,
  expectedBotId: string,
): ParsedMediaMessage | undefined {
  const body = frame.body;
  if (!body || body.msgtype !== "mixed" || !Array.isArray(body.mixed?.msg_item)) {
    return undefined;
  }
  const parsed = parseMessageEnvelope(frame, body, expectedBotId);
  if (!parsed) return undefined;
  const text = body.mixed.msg_item
    .flatMap((item) => (item.msgtype === "text" && item.text?.content ? [item.text.content] : []))
    .join("\n")
    .trim();
  const images = body.mixed.msg_item.flatMap((item) =>
    item.msgtype === "image" && item.image?.url
      ? [{ url: item.image.url, aesKey: item.image.aeskey }]
      : [],
  );
  if (!text && images.length === 0) return undefined;
  return {
    ...parsed,
    event: {
      ...parsed.event,
      content: text || "请分析我发送的图片。",
    },
    images,
  };
}

function parseMessageEnvelope(
  frame: Pick<WsFrame, "headers">,
  body: BaseMessage,
  expectedBotId: string,
): Omit<ParsedMediaMessage, "images"> | undefined {
  if (body.aibotid !== expectedBotId || !frame.headers?.req_id) return undefined;
  const messageId = body.msgid;
  const userId = body.from?.userid;
  const chatId = body.chattype === "group" ? body.chatid : userId;
  if (!messageId || !userId || !chatId) return undefined;
  return {
    frame,
    chatId,
    event: {
      type: "message",
      platform: "wecom",
      message_id: messageId,
      chat_id: chatId,
      chat_type: body.chattype === "group" ? "group" : "direct",
      user_id: userId,
      content: "",
      mentioned_bot: true,
      quoted_text: body.quote === undefined ? undefined : (extractQuotedText(body.quote) ?? ""),
    },
  };
}

export function parseApprovalCardEvent(
  frame: ApprovalEventFrame,
  expectedBotId: string,
): ParsedApprovalCardEvent | undefined {
  const body = frame.body;
  const event = body?.event as
    | (Partial<TemplateCardEventData> & {
        template_card_event?: Pick<TemplateCardEventData, "event_key" | "task_id">;
      })
    | undefined;
  if (
    !body ||
    body.aibotid !== expectedBotId ||
    body.msgtype !== "event" ||
    (event?.eventtype !== undefined && event.eventtype !== EventType.TemplateCardEvent) ||
    !frame.headers?.req_id
  ) {
    return undefined;
  }
  // The real WeCom callback nests interaction data under
  // event.template_card_event. SDK 1.0.7 still declares the older flat shape,
  // so accept both while applying the same key/task validation below.
  const cardEvent = event?.template_card_event ?? event;
  const eventKey = cardEvent?.event_key;
  const taskId = cardEvent?.task_id;
  const match = eventKey?.match(/^codex_(approve|deny)_([0-9A-F]{6})$/);
  if (!match || !taskId || !taskId.endsWith(`_${match[2]}`)) return undefined;

  // Some WeCom callback variants omit msgid/chattype even though the current
  // SDK types mark them as present. req_id is unique for this callback and is a
  // safe deduplication fallback. A chatid always identifies a group callback.
  const messageId = body.msgid || frame.headers.req_id;
  const userId = body.from?.userid;
  const isGroup = body.chattype === "group" || Boolean(body.chatid);
  const chatId = isGroup ? body.chatid : userId;
  if (!messageId || !userId || !chatId) return undefined;

  return {
    frame,
    chatId,
    taskId,
    userId,
    event: {
      type: "message",
      platform: "wecom",
      message_id: messageId,
      chat_id: chatId,
      chat_type: isGroup ? "group" : "direct",
      user_id: userId,
      content: `/${match[1]} ${match[2]}`,
      mentioned_bot: true,
      card_task_id: taskId,
    },
  };
}

function extractQuotedText(quote: TextMessage["quote"]): string | undefined {
  if (!quote) return undefined;
  if (quote.text?.content) return quote.text.content;
  if (quote.voice?.content) return quote.voice.content;
  if (quote.mixed) {
    const text = quote.mixed.msg_item
      .flatMap((item) => (item.text?.content ? [item.text.content] : []))
      .join("\n");
    return text || undefined;
  }
  return undefined;
}

export function isStreamExpiredError(error: unknown): boolean {
  if (typeof error === "object" && error !== null) {
    const candidate = error as { errcode?: unknown; errmsg?: unknown; message?: unknown };
    if (candidate.errcode === STREAM_EXPIRED_ERROR_CODE) return true;
    if (String(candidate.errmsg ?? "").includes(String(STREAM_EXPIRED_ERROR_CODE))) return true;
    if (String(candidate.message ?? "").includes(String(STREAM_EXPIRED_ERROR_CODE))) return true;
  }
  return String(error).includes(String(STREAM_EXPIRED_ERROR_CODE));
}

export function clipStreamContent(content: string): string {
  if (Buffer.byteLength(content, "utf8") <= MAX_STREAM_CONTENT_BYTES) return content;

  const suffix = "\n\n（回复超过企业微信单条消息限制，已截断。）";
  const budget = MAX_STREAM_CONTENT_BYTES - Buffer.byteLength(suffix, "utf8");
  let used = 0;
  let clipped = "";
  for (const character of content) {
    const bytes = Buffer.byteLength(character, "utf8");
    if (used + bytes > budget) break;
    clipped += character;
    used += bytes;
  }
  return clipped + suffix;
}

export class ReplyManager {
  private readonly contexts = new Map<string, ReplyContext>();
  private readonly approvalEvents = new Map<string, ApprovalEventContext>();

  constructor(private readonly client: ReplyClient) {}

  register(messageId: string, frame: Pick<WsFrame, "headers">, chatId: string): void {
    this.contexts.set(messageId, {
      frame,
      chatId,
      streamId: generateReqId("codex"),
      expired: false,
    });
  }

  has(messageId: string): boolean {
    return this.contexts.has(messageId);
  }

  registerApprovalEvent(parsed: ParsedApprovalCardEvent): void {
    this.approvalEvents.set(parsed.event.message_id, {
      frame: parsed.frame,
      taskId: parsed.taskId,
      userId: parsed.userId,
    });
    setTimeout(() => this.approvalEvents.delete(parsed.event.message_id), 10_000).unref();
  }

  async handle(command: BridgeCommand): Promise<void> {
    if (command.type === "approval") {
      await this.handleApproval(command);
      return;
    }
    if (command.type === "approval_result") {
      await this.handleApprovalResult(command);
      return;
    }
    const context = this.contexts.get(command.message_id);
    const content = clipStreamContent(command.content);
    if (!context) {
      await this.sendActive(command.chat_id, content);
      return;
    }
    if (context.expired) {
      if (command.finished) {
        await this.sendActive(context.chatId, content);
        this.contexts.delete(command.message_id);
      }
      return;
    }

    try {
      // The SDK drops an intermediate update while the previous ACK is still
      // pending, but always queues the final frame. Each update contains the
      // full accumulated answer, so dropping an intermediate frame is safe.
      await this.client.replyStreamNonBlocking(
        context.frame,
        context.streamId,
        content,
        command.finished,
      );
      if (command.finished) this.contexts.delete(command.message_id);
    } catch (error) {
      if (!isStreamExpiredError(error)) throw error;
      context.expired = true;
      if (command.finished) {
        await this.sendActive(context.chatId, content);
        this.contexts.delete(command.message_id);
      }
    }
  }

  private async handleApproval(command: ApprovalCommand): Promise<void> {
    const context = this.contexts.get(command.message_id);
    const content = clipStreamContent(command.content);
    const card = approvalCard(command);
    if (!context || context.expired) {
      await this.sendActive(command.chat_id, content);
      await this.sendCard(command.chat_id, card);
      this.contexts.delete(command.message_id);
      return;
    }

    try {
      await this.client.replyStreamNonBlocking(context.frame, context.streamId, content, true);
      await this.sendCard(context.chatId, card);
      this.contexts.delete(command.message_id);
    } catch (error) {
      if (!isStreamExpiredError(error)) throw error;
      await this.sendActive(context.chatId, content);
      await this.sendCard(context.chatId, card);
      this.contexts.delete(command.message_id);
    }
  }

  private async handleApprovalResult(command: ApprovalResultCommand): Promise<void> {
    const context = this.approvalEvents.get(command.message_id);
    if (!context || context.taskId !== command.task_id) {
      await this.sendActive(command.chat_id, command.content);
      return;
    }
    const title =
      command.status === "approved"
        ? "审批已通过 ✅"
        : command.status === "denied"
          ? "审批已拒绝 ❌"
          : "审批未生效 ⚠️";
    const card: TemplateCard = {
      card_type: "text_notice",
      source: {
        desc: "Codex 审批",
        desc_color: command.status === "approved" ? 3 : 2,
      },
      main_title: { title, desc: `任务 ${command.task_id.slice(-6)}` },
      sub_title_text: clipCardText(command.content, 112),
      // WeCom requires card_action even when the result card is informational.
      card_action: { type: 1, url: "https://work.weixin.qq.com" },
      task_id: command.task_id,
    };
    const userIds = command.status === "error" ? [context.userId] : undefined;
    await this.client.updateTemplateCard(context.frame, card, userIds);
    this.approvalEvents.delete(command.message_id);
  }

  private async sendActive(chatId: string, content: string): Promise<void> {
    await this.client.sendMessage(chatId, {
      msgtype: "markdown",
      markdown: { content },
    });
  }

  private async sendCard(chatId: string, card: TemplateCard): Promise<void> {
    await this.client.sendMessage(chatId, {
      msgtype: "template_card",
      template_card: card,
    });
  }
}

function approvalCard(command: ApprovalCommand): TemplateCard {
  return {
    card_type: "button_interaction",
    source: { desc: "Codex 审批", desc_color: 2 },
    main_title: {
      title: clipCardText(command.title, 26),
      desc: `审批编号 ${command.token}`,
    },
    sub_title_text: "请核对消息中的操作详情。仅审批管理员的选择会生效。",
    button_list: [
      { text: "批准", key: `codex_approve_${command.token}`, style: 1 },
      { text: "拒绝", key: `codex_deny_${command.token}`, style: 2 },
    ],
    task_id: command.task_id,
  };
}

function clipCardText(text: string, maxCharacters: number): string {
  return [...text].slice(0, maxCharacters).join("");
}
