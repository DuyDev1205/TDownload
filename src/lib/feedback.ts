// Góp ý tính năng / báo lỗi: gửi lên máy chủ Google Apps Script (feedback/Code.gs), lưu trong Google Sheet của
// người làm app. Người gửi theo dõi trạng thái bằng id; người làm app duyệt ngay trong VidSaver bằng mã quản trị.

import { invoke } from "@tauri-apps/api/core";
import { load, save } from "./storage";

/** URL ứng dụng web của Apps Script (Triển khai → Ứng dụng web). Rỗng nghĩa là chưa thiết lập. */
export const FEEDBACK_URL = "";

export type FeedbackType = "feature" | "bug";
export type FeedbackStatus = "pending" | "accepted" | "rejected";

export const STATUS_NAME: Record<FeedbackStatus, string> = {
  pending: "Đang chờ",
  accepted: "Chấp nhận",
  rejected: "Từ chối",
};

export const TYPE_NAME: Record<FeedbackType, string> = {
  feature: "Góp ý tính năng",
  bug: "Báo lỗi",
};

export interface ImageData {
  mime: string;
  /** Nội dung ảnh dạng base64 (không có tiền tố data:). */
  data: string;
}

/** Góp ý mình đã gửi, lưu trên máy để theo dõi trạng thái. */
export interface SentItem {
  id: string;
  type: FeedbackType;
  title: string;
  createdAt: number;
  status: FeedbackStatus;
  reply: string;
  /** Đã xem kết quả mới nhất chưa (để hiện chấm đỏ trên tab). */
  seen: boolean;
}

/** Một góp ý trong danh sách quản trị. */
export interface AdminItem {
  id: string;
  createdAt: string;
  type: FeedbackType;
  title: string;
  description: string;
  contact: string;
  appVersion: string;
  os: string;
  images: string[];
  status: FeedbackStatus;
  reply: string;
  updatedAt: string;
}

const request = <T>(body: Record<string, unknown>) => invoke<T>("feedback_request", { url: FEEDBACK_URL, body });

export const submitFeedback = (args: {
  type: FeedbackType;
  title: string;
  description: string;
  contact: string;
  appVersion: string;
  os: string;
  images: ImageData[];
}) => request<{ id: string }>({ action: "submit", ...args });

export const fetchStatuses = (ids: string[]) =>
  request<{ id: string; status: FeedbackStatus; reply: string; updatedAt: string }[]>({ action: "status", ids });

export const adminList = (adminKey: string) => request<AdminItem[]>({ action: "list", adminKey });
export const adminImage = (adminKey: string, fileId: string) => request<ImageData>({ action: "image", adminKey, fileId });
export const adminReview = (adminKey: string, id: string, status: FeedbackStatus, reply: string) =>
  request<{ id: string; status: FeedbackStatus; updatedAt: string }>({ action: "review", adminKey, id, status, reply });

export function loadSent(): SentItem[] {
  try {
    return JSON.parse(load("feedbackSent", "[]")) as SentItem[];
  } catch {
    return [];
  }
}

export function saveSent(items: SentItem[]) {
  save("feedbackSent", JSON.stringify(items.slice(0, 100)));
}

/** Ảnh chụp màn hình thường rất nặng: thu về tối đa 1920 px và nén JPEG nếu file lớn hơn 800 KB. */
export async function readImage(file: Blob): Promise<ImageData & { url: string }> {
  let blob = file;
  if (file.size > 800 * 1024) {
    const bitmap = await createImageBitmap(file);
    const scale = Math.min(1, 1920 / Math.max(bitmap.width, bitmap.height));
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(bitmap.width * scale);
    canvas.height = Math.round(bitmap.height * scale);
    canvas.getContext("2d")!.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    blob = await new Promise<Blob>((resolve) => canvas.toBlob((b) => resolve(b ?? file), "image/jpeg", 0.85));
  }
  const url = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
  return { mime: blob.type || "image/png", data: url.split(",")[1] ?? "", url };
}

export const imageUrl = (img: ImageData) => `data:${img.mime};base64,${img.data}`;
