import assert from "node:assert/strict";
import test from "node:test";
import {
  EventType,
  MessageType,
  type EventMessageWith,
  type ImageMessage,
  type MixedMessage,
  type SendMsgBody,
  type TemplateCard,
  type TemplateCardEventData,
  type TextMessage,
  type WsFrame,
} from "@wecom/aibot-node-sdk";
import {
  clipStreamContent,
  isStreamExpiredError,
  parseApprovalCardEvent,
  parseImageMessage,
  parseMixedMessage,
  parseTextMessage,
  ReplyManager,
  type ReplyClient,
} from "./adapter.js";

const frame = (overrides: Partial<TextMessage> = {}): WsFrame<TextMessage> => ({
  cmd: "aibot_msg_callback",
  headers: { req_id: "req-1" },
  body: {
    msgid: "msg-1",
    aibotid: "bot-1",
    chatid: "chat-1",
    chattype: "group",
    from: { userid: "user-1" },
    msgtype: MessageType.Text,
    text: { content: "  investigate login failure  " },
    ...overrides,
  },
});

const imageFrame = (overrides: Partial<ImageMessage> = {}): WsFrame<ImageMessage> => ({
  cmd: "aibot_msg_callback",
  headers: { req_id: "image-req-1" },
  body: {
    msgid: "image-msg-1",
    aibotid: "bot-1",
    chattype: "single",
    from: { userid: "user-1" },
    msgtype: MessageType.Image,
    image: { url: "https://example.invalid/encrypted", aeskey: "aes-key" },
    ...overrides,
  },
});

const mixedFrame = (overrides: Partial<MixedMessage> = {}): WsFrame<MixedMessage> => ({
  cmd: "aibot_msg_callback",
  headers: { req_id: "mixed-req-1" },
  body: {
    msgid: "mixed-msg-1",
    aibotid: "bot-1",
    chatid: "chat-1",
    chattype: "group",
    from: { userid: "user-1" },
    msgtype: MessageType.Mixed,
    mixed: {
      msg_item: [
        { msgtype: "text", text: { content: "看看这个报错" } },
        {
          msgtype: "image",
          image: { url: "https://example.invalid/encrypted", aeskey: "aes-key" },
        },
      ],
    },
    ...overrides,
  },
});

const approvalEventFrame = (
  overrides: Partial<EventMessageWith<TemplateCardEventData>> = {},
): WsFrame<EventMessageWith<TemplateCardEventData>> => ({
  cmd: "aibot_event_callback",
  headers: { req_id: "event-req-1" },
  body: {
    msgid: "event-msg-1",
    create_time: Date.now(),
    aibotid: "bot-1",
    chatid: "chat-1",
    chattype: "group",
    from: { userid: "admin-1" },
    msgtype: "event",
    event: {
      eventtype: EventType.TemplateCardEvent,
      event_key: "codex_approve_ABC123",
      task_id: "codex_thread-1_ABC123",
    },
    ...overrides,
  },
});

class FakeClient implements ReplyClient {
  streams: Array<{ streamId: string; content: string; finished: boolean }> = [];
  active: Array<{ chatId: string; body: SendMsgBody }> = [];
  updates: Array<{
    card: TemplateCard;
    userIds?: string[];
  }> = [];
  streamError?: unknown;

  async replyStreamNonBlocking(
    _frame: Pick<WsFrame, "headers">,
    streamId: string,
    content: string,
    finished = false,
  ): Promise<WsFrame | "skipped"> {
    if (this.streamError !== undefined) throw this.streamError;
    this.streams.push({ streamId, content, finished });
    return { headers: { req_id: "ack" }, errcode: 0 };
  }

  async sendMessage(chatId: string, body: SendMsgBody): Promise<WsFrame> {
    this.active.push({ chatId, body });
    return { headers: { req_id: "active-ack" }, errcode: 0 };
  }

  async updateTemplateCard(
    _frame: Pick<WsFrame, "headers">,
    card: TemplateCard,
    userIds?: string[],
  ): Promise<WsFrame> {
    this.updates.push({ card, userIds });
    return { headers: { req_id: "update-ack" }, errcode: 0 };
  }
}

test("parses a valid group callback and preserves the original text", () => {
  const parsed = parseTextMessage(frame(), "bot-1");
  assert.ok(parsed);
  assert.equal(parsed.event.chat_type, "group");
  assert.equal(parsed.event.chat_id, "chat-1");
  assert.equal(parsed.event.user_id, "user-1");
  assert.equal(parsed.event.content, "  investigate login failure  ");
  assert.equal(parsed.event.mentioned_bot, true);
});

test("rejects callbacks for another bot or malformed group callbacks", () => {
  assert.equal(parseTextMessage(frame(), "bot-2"), undefined);
  assert.equal(parseTextMessage(frame({ chatid: undefined }), "bot-1"), undefined);
  assert.equal(
    parseTextMessage(frame({ text: { content: "   " } }), "bot-1"),
    undefined,
  );
});

test("parses a direct message using the sender as the chat id", () => {
  const parsed = parseTextMessage(
    frame({ chatid: undefined, chattype: "single" }),
    "bot-1",
  );
  assert.ok(parsed);
  assert.equal(parsed.event.chat_type, "direct");
  assert.equal(parsed.event.chat_id, "user-1");
});

test("preserves quoted text for task routing", () => {
  const parsed = parseTextMessage(
    frame({
      quote: {
        msgtype: "text",
        text: { content: "previous answer\n\n[Codex任务:R8K3P2Q7W9XZ]" },
      },
    }),
    "bot-1",
  );
  assert.ok(parsed);
  assert.equal(parsed.event.quoted_text, "previous answer\n\n[Codex任务:R8K3P2Q7W9XZ]");
});

test("distinguishes an unroutable quote from an unquoted message", () => {
  const parsed = parseTextMessage(
    frame({
      quote: {
        msgtype: "image",
        image: { url: "https://example.invalid/encrypted-image" },
      },
    }),
    "bot-1",
  );
  assert.ok(parsed);
  assert.equal(parsed.event.quoted_text, "");
  assert.equal(parseTextMessage(frame(), "bot-1")?.event.quoted_text, undefined);
});

test("parses a direct image with its encrypted download metadata", () => {
  const parsed = parseImageMessage(imageFrame(), "bot-1");
  assert.ok(parsed);
  assert.equal(parsed.event.chat_type, "direct");
  assert.equal(parsed.event.content, "请分析我发送的图片。");
  assert.deepEqual(parsed.images, [
    { url: "https://example.invalid/encrypted", aesKey: "aes-key" },
  ]);
  assert.equal(parseImageMessage(imageFrame(), "another-bot"), undefined);
});

test("parses mixed text and images while preserving quote routing", () => {
  const parsed = parseMixedMessage(
    mixedFrame({
      quote: {
        msgtype: "text",
        text: { content: "previous\n\n[Codex任务:R8K3P2Q7W9XZ]" },
      },
    }),
    "bot-1",
  );
  assert.ok(parsed);
  assert.equal(parsed.event.chat_type, "group");
  assert.equal(parsed.event.content, "看看这个报错");
  assert.equal(parsed.event.quoted_text, "previous\n\n[Codex任务:R8K3P2Q7W9XZ]");
  assert.equal(parsed.images.length, 1);
});

test("parses approval card clicks into existing approval commands", () => {
  const parsed = parseApprovalCardEvent(approvalEventFrame(), "bot-1");
  assert.ok(parsed);
  assert.equal(parsed.event.content, "/approve ABC123");
  assert.equal(parsed.event.card_task_id, "codex_thread-1_ABC123");
  assert.equal(parsed.event.user_id, "admin-1");

  const mismatchedTask = approvalEventFrame({
    event: {
      eventtype: EventType.TemplateCardEvent,
      event_key: "codex_approve_ABC123",
      task_id: "codex_thread-1_FFFFFF",
    },
  });
  assert.equal(parseApprovalCardEvent(mismatchedTask, "bot-1"), undefined);
  assert.equal(parseApprovalCardEvent(approvalEventFrame(), "another-bot"), undefined);
});

test("parses the real nested approval callback with omitted envelope fields", () => {
  const variant = approvalEventFrame();
  assert.ok(variant.body);
  delete (variant.body as { msgid?: string }).msgid;
  delete variant.body.chattype;
  variant.body.event = {
    eventtype: EventType.TemplateCardEvent,
    template_card_event: {
      event_key: "codex_approve_ABC123",
      task_id: "codex_thread-1_ABC123",
    },
  } as TemplateCardEventData;

  const parsed = parseApprovalCardEvent(variant, "bot-1");
  assert.ok(parsed);
  assert.equal(parsed.event.message_id, "event-req-1");
  assert.equal(parsed.event.chat_type, "group");
  assert.equal(parsed.event.chat_id, "chat-1");
});

test("finishes the current reply stream and actively sends an approval card", async () => {
  const client = new FakeClient();
  const replies = new ReplyManager(client);
  replies.register("msg-1", frame(), "chat-1");

  await replies.handle({
    type: "approval",
    platform: "wecom",
    message_id: "msg-1",
    chat_id: "chat-1",
    token: "ABC123",
    task_id: "codex_thread-1_ABC123",
    title: "Codex 命令执行审批",
    content: "approval details",
  });

  assert.equal(client.streams.length, 1);
  assert.deepEqual(client.streams[0], {
    streamId: client.streams[0]?.streamId,
    content: "approval details",
    finished: true,
  });
  assert.equal(client.active.length, 1);
  assert.equal(client.active[0]?.body.msgtype, "template_card");
  const card =
    client.active[0]?.body.msgtype === "template_card"
      ? client.active[0].body.template_card
      : undefined;
  assert.deepEqual(card?.button_list, [
    { text: "批准", key: "codex_approve_ABC123", style: 1 },
    { text: "拒绝", key: "codex_deny_ABC123", style: 2 },
  ]);
  assert.equal(replies.has("msg-1"), false);
});

test("actively sends approval details and a card after the reply stream expires", async () => {
  const client = new FakeClient();
  const replies = new ReplyManager(client);

  await replies.handle({
    type: "approval",
    platform: "wecom",
    message_id: "missing-message",
    chat_id: "chat-1",
    token: "ABC123",
    task_id: "codex_message-1_ABC123",
    title: "Codex 文件修改审批",
    content: "approval details",
  });

  assert.equal(client.active.length, 2);
  assert.equal(client.active[0]?.body.msgtype, "markdown");
  assert.equal(client.active[1]?.body.msgtype, "template_card");
});

test("updates the clicked approval card after core authorization", async () => {
  const client = new FakeClient();
  const replies = new ReplyManager(client);
  const parsed = parseApprovalCardEvent(approvalEventFrame(), "bot-1");
  assert.ok(parsed);
  replies.registerApprovalEvent(parsed);

  await replies.handle({
    type: "approval_result",
    platform: "wecom",
    message_id: "event-msg-1",
    chat_id: "chat-1",
    task_id: "codex_thread-1_ABC123",
    status: "approved",
    content: "Codex 请求已处理，任务继续执行。",
  });

  assert.equal(client.updates.length, 1);
  assert.equal(client.updates[0]?.card.main_title?.title, "审批已通过 ✅");
  assert.deepEqual(client.updates[0]?.card.card_action, {
    type: 1,
    url: "https://work.weixin.qq.com",
  });
  assert.equal(client.updates[0]?.userIds, undefined);
});

test("shows a card error only to the unauthorized clicker", async () => {
  const client = new FakeClient();
  const replies = new ReplyManager(client);
  const parsed = parseApprovalCardEvent(approvalEventFrame(), "bot-1");
  assert.ok(parsed);
  replies.registerApprovalEvent(parsed);

  await replies.handle({
    type: "approval_result",
    platform: "wecom",
    message_id: "event-msg-1",
    chat_id: "chat-1",
    task_id: "codex_thread-1_ABC123",
    status: "error",
    content: "只有配置的审批管理员可以批准或拒绝这个操作。",
  });

  assert.equal(client.updates[0]?.card.main_title?.title, "审批未生效 ⚠️");
  assert.deepEqual(client.updates[0]?.userIds, ["admin-1"]);
});

test("recognizes the SDK's non-Error stream expiry acknowledgement", () => {
  assert.equal(isStreamExpiredError({ errcode: 846608, errmsg: "stream expired" }), true);
  assert.equal(isStreamExpiredError(new Error("reply failed: 846608")), true);
  assert.equal(isStreamExpiredError({ errcode: 400, errmsg: "bad request" }), false);
});

test("uses non-blocking stream updates and keeps one stream id", async () => {
  const client = new FakeClient();
  const replies = new ReplyManager(client);
  replies.register("msg-1", frame(), "chat-1");

  await replies.handle({
    type: "reply",
    platform: "wecom",
    message_id: "msg-1",
    chat_id: "untrusted-chat-id",
    content: "working",
    finished: false,
  });
  await replies.handle({
    type: "reply",
    platform: "wecom",
    message_id: "msg-1",
    chat_id: "untrusted-chat-id",
    content: "done",
    finished: true,
  });

  assert.equal(client.streams.length, 2);
  assert.equal(client.streams[0]?.streamId, client.streams[1]?.streamId);
  assert.deepEqual(client.streams.map((entry) => entry.finished), [false, true]);
  assert.equal(replies.has("msg-1"), false);
});

test("falls back to an active message when the callback stream expires", async () => {
  const client = new FakeClient();
  client.streamError = { errcode: 846608, errmsg: "stream expired" };
  const replies = new ReplyManager(client);
  replies.register("msg-1", frame(), "chat-1");

  await replies.handle({
    type: "reply",
    platform: "wecom",
    message_id: "msg-1",
    chat_id: "wrong-chat",
    content: "final answer",
    finished: true,
  });

  assert.equal(client.active.length, 1);
  assert.equal(client.active[0]?.chatId, "chat-1");
  assert.deepEqual(client.active[0]?.body, {
    msgtype: "markdown",
    markdown: { content: "final answer" },
  });
  assert.equal(replies.has("msg-1"), false);
});

test("waits for the final accumulated answer after an intermediate stream expires", async () => {
  const client = new FakeClient();
  client.streamError = { errcode: 846608, errmsg: "stream expired" };
  const replies = new ReplyManager(client);
  replies.register("msg-1", frame(), "chat-1");

  await replies.handle({
    type: "reply",
    platform: "wecom",
    message_id: "msg-1",
    chat_id: "wrong-chat",
    content: "partial",
    finished: false,
  });
  assert.equal(client.active.length, 0);
  assert.equal(replies.has("msg-1"), true);

  client.streamError = undefined;
  await replies.handle({
    type: "reply",
    platform: "wecom",
    message_id: "msg-1",
    chat_id: "wrong-chat",
    content: "complete accumulated answer",
    finished: true,
  });
  assert.equal(client.active[0]?.chatId, "chat-1");
  assert.deepEqual(client.active[0]?.body, {
    msgtype: "markdown",
    markdown: { content: "complete accumulated answer" },
  });
  assert.equal(replies.has("msg-1"), false);
});

test("clips UTF-8 content without splitting a character", () => {
  const clipped = clipStreamContent("中".repeat(7_000));
  assert.ok(Buffer.byteLength(clipped, "utf8") <= 20_480);
  assert.equal(clipped.endsWith("已截断。）"), true);
});
