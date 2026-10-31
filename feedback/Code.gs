/**
 * Máy chủ góp ý / báo lỗi của VidSaver, chạy miễn phí bằng Google Apps Script gắn với một Google Sheet.
 * Mỗi góp ý là một dòng trong trang "Góp ý"; ảnh đính kèm lưu trong một thư mục Google Drive riêng tư.
 *
 * Cài đặt (một lần):
 *   1. Tạo Google Sheet mới → Tiện ích mở rộng → Apps Script → xoá code mẫu, dán toàn bộ file này → Lưu.
 *   2. Chọn hàm `setup` → Chạy → cấp quyền. Mở "Nhật ký thực thi" để lấy MÃ QUẢN TRỊ (ADMIN_KEY).
 *   3. Triển khai → Tùy chọn triển khai mới → loại "Ứng dụng web":
 *        Thực thi với tư cách: Tôi · Người có quyền truy cập: Bất kỳ ai → Triển khai → copy URL ứng dụng web.
 *   4. Đưa URL đó vào FEEDBACK_URL trong src/lib/feedback.ts. Mã quản trị nhập trong VidSaver (tab Góp ý).
 * Sửa code này sau khi đã triển khai: Triển khai → Quản lý triển khai → sửa → Phiên bản mới (giữ nguyên URL).
 */

const SHEET_NAME = "Góp ý";
const HEADERS = ["id", "createdAt", "type", "title", "description", "contact", "appVersion", "os", "images", "status", "reply", "updatedAt"];
const STATUSES = ["pending", "accepted", "rejected"];
const MAX_IMAGES = 5;
const MAX_IMAGE_BYTES = 5 * 1024 * 1024;

// Mỗi máy cài VidSaver là một dòng: mã cài đặt ngẫu nhiên, lần đầu / lần gần nhất mở app, số ngày đã dùng.
// internal = máy đã đăng nhập Quản trị (máy của người làm app), không tính vào thống kê.
// installs = số lượt tải của máy đó: 1 lần cài đầu tiên, cộng 1 mỗi lần lên phiên bản mới (kể cả tự cập nhật).
// name = tên người dùng tự điền trong app (không bắt buộc).
const USERS_SHEET = "Người dùng";
const USER_HEADERS = ["installId", "firstSeen", "lastSeen", "appVersion", "os", "days", "internal", "installs", "name"];
const MAX_MACHINES = 300;
const DAY_MS = 24 * 60 * 60 * 1000;

function setup() {
  const sheet = getSheet_();
  getSheet_(USERS_SHEET, USER_HEADERS);
  const statusColumn = HEADERS.indexOf("status") + 1;
  const rule = SpreadsheetApp.newDataValidation().requireValueInList(STATUSES, true).build();
  sheet.getRange(2, statusColumn, sheet.getMaxRows() - 1, 1).setDataValidation(rule);

  const props = PropertiesService.getScriptProperties();
  if (!props.getProperty("FOLDER_ID")) props.setProperty("FOLDER_ID", DriveApp.createFolder("VidSaver - ảnh góp ý").getId());
  if (!props.getProperty("ADMIN_KEY")) props.setProperty("ADMIN_KEY", Utilities.getUuid());
  Logger.log("MÃ QUẢN TRỊ (nhập vào VidSaver): " + props.getProperty("ADMIN_KEY"));
}

function doPost(e) {
  try {
    const req = JSON.parse(e.postData.contents);
    const handlers = { submit: submit_, status: status_, list: list_, image: image_, review: review_, ping: ping_, stats: stats_ };
    const handler = handlers[req.action];
    if (!handler) throw new Error("Yêu cầu không hợp lệ.");
    return json_({ ok: true, data: handler(req) });
  } catch (err) {
    return json_({ ok: false, error: String((err && err.message) || err) });
  }
}

/** Người dùng gửi góp ý/báo lỗi. Trả về id để app theo dõi trạng thái. */
function submit_(req) {
  const title = text_(req.title, 200);
  if (!title) throw new Error("Thiếu tiêu đề.");
  const images = (req.images || []).slice(0, MAX_IMAGES);
  const id = Utilities.getUuid();
  const folder = DriveApp.getFolderById(prop_("FOLDER_ID"));
  const fileIds = images.map(function (img, i) {
    const bytes = Utilities.base64Decode(String(img.data || ""));
    if (bytes.length > MAX_IMAGE_BYTES) throw new Error("Ảnh quá lớn (tối đa 5 MB mỗi ảnh).");
    const mime = /^image\/(png|jpeg|webp|gif)$/.test(img.mime) ? img.mime : "image/png";
    return folder.createFile(Utilities.newBlob(bytes, mime, id + "-" + (i + 1) + "." + mime.split("/")[1])).getId();
  });
  const now = new Date().toISOString();
  withLock_(function () {
    getSheet_().appendRow([
      id,
      now,
      req.type === "bug" ? "bug" : "feature",
      title,
      text_(req.description, 5000),
      text_(req.contact, 200),
      text_(req.appVersion, 30),
      text_(req.os, 80),
      fileIds.join(","),
      "pending",
      "",
      now,
    ]);
  });
  return { id: id };
}

/** Người gửi hỏi trạng thái các góp ý của mình (biết id mới hỏi được). */
function status_(req) {
  const ids = {};
  (req.ids || []).slice(0, 300).forEach(function (id) {
    ids[id] = true;
  });
  return rows_()
    .filter(function (r) {
      return ids[r.id];
    })
    .map(function (r) {
      return { id: r.id, status: r.status, reply: r.reply, updatedAt: r.updatedAt };
    });
}

/** Quản trị: toàn bộ góp ý, mới nhất trước. */
function list_(req) {
  checkAdmin_(req);
  return rows_().reverse();
}

/** Quản trị: lấy một ảnh đính kèm (chỉ ảnh trong thư mục góp ý). */
function image_(req) {
  checkAdmin_(req);
  const file = DriveApp.getFileById(String(req.fileId));
  const parents = file.getParents();
  if (!parents.hasNext() || parents.next().getId() !== prop_("FOLDER_ID")) throw new Error("Không tìm thấy ảnh.");
  const blob = file.getBlob();
  return { mime: blob.getContentType(), data: Utilities.base64Encode(blob.getBytes()) };
}

/** Quản trị: chấp nhận / từ chối kèm lời nhắn cho người gửi. */
function review_(req) {
  checkAdmin_(req);
  if (STATUSES.indexOf(req.status) < 0) throw new Error("Trạng thái không hợp lệ.");
  return withLock_(function () {
    const sheet = getSheet_();
    const ids = sheet.getRange(2, 1, Math.max(sheet.getLastRow() - 1, 1), 1).getValues();
    for (let i = 0; i < ids.length; i++) {
      if (ids[i][0] === req.id) {
        const row = i + 2;
        const now = new Date().toISOString();
        sheet.getRange(row, HEADERS.indexOf("status") + 1).setValue(req.status);
        sheet.getRange(row, HEADERS.indexOf("reply") + 1).setValue(text_(req.reply, 2000));
        sheet.getRange(row, HEADERS.indexOf("updatedAt") + 1).setValue(now);
        return { id: req.id, status: req.status, updatedAt: now };
      }
    }
    throw new Error("Không tìm thấy góp ý.");
  });
}

/** App báo về mỗi ngày một lần khi được mở. Máy nào gửi kèm đúng mã quản trị thì đánh dấu là máy nội bộ. */
function ping_(req) {
  const id = String(req.installId || "");
  if (!/^[0-9a-f-]{36}$/i.test(id)) throw new Error("Mã cài đặt không hợp lệ.");
  const internal = !!req.adminKey && req.adminKey === PropertiesService.getScriptProperties().getProperty("ADMIN_KEY");
  const version = text_(req.appVersion, 30);
  const os = text_(req.os, 80);
  const name = text_(req.name, 80);
  const now = new Date();
  withLock_(function () {
    const sheet = getSheet_(USERS_SHEET, USER_HEADERS);
    const found = sheet.getRange("A:A").createTextFinder(id).matchEntireCell(true).findNext();
    if (!found) {
      sheet.appendRow([id, now, now, version, os, 1, internal, 1, name]);
      return;
    }
    const row = found.getRow();
    const old = sheet.getRange(row, 1, 1, USER_HEADERS.length).getValues()[0];
    const newDay = new Date(old[2]).toDateString() !== now.toDateString();
    const updated = String(old[3]) !== version;
    sheet.getRange(row, 3, 1, 7).setValues([
      [
        now,
        version,
        os,
        (Number(old[5]) || 0) + (newDay ? 1 : 0),
        // Đã từng là máy nội bộ thì giữ nguyên, kể cả khi sau đó thoát Quản trị.
        internal || old[6] === true,
        (Number(old[7]) || 1) + (updated ? 1 : 0),
        name,
      ],
    ]);
  });
  return {};
}

/** Quản trị: số máy đang dùng, chỉ tính máy thật (bỏ máy nội bộ). */
function stats_(req) {
  checkAdmin_(req);
  const sheet = getSheet_(USERS_SHEET, USER_HEADERS);
  const rows = sheet.getLastRow() < 2 ? [] : sheet.getRange(2, 1, sheet.getLastRow() - 1, USER_HEADERS.length).getValues();
  const now = Date.now();
  const out = { total: 0, downloads: 0, day: 0, week: 0, month: 0, newWeek: 0, internal: 0, versions: {}, systems: {}, machines: [] };
  rows.forEach(function (r) {
    if (!r[0]) return;
    if (r[6] === true) {
      out.internal++;
      return;
    }
    const idle = now - new Date(r[2]).getTime();
    out.total++;
    out.downloads += Number(r[7]) || 1;
    out.machines.push({
      name: String(r[8] || ""),
      appVersion: String(r[3]),
      os: String(r[4]),
      firstSeen: new Date(r[1]).toISOString(),
      lastSeen: new Date(r[2]).toISOString(),
      installs: Number(r[7]) || 1,
    });
    if (idle < DAY_MS) out.day++;
    if (idle < 7 * DAY_MS) out.week++;
    if (now - new Date(r[1]).getTime() < 7 * DAY_MS) out.newWeek++;
    if (idle < 30 * DAY_MS) {
      out.month++;
      const version = String(r[3]);
      const system = String(r[4]).split(" ")[0];
      out.versions[version] = (out.versions[version] || 0) + 1;
      out.systems[system] = (out.systems[system] || 0) + 1;
    }
  });
  // Mới dùng gần đây nhất trước.
  out.machines.sort(function (a, b) {
    return a.lastSeen < b.lastSeen ? 1 : -1;
  });
  out.machines = out.machines.slice(0, MAX_MACHINES);
  return out;
}

function rows_() {
  const sheet = getSheet_();
  if (sheet.getLastRow() < 2) return [];
  return sheet
    .getRange(2, 1, sheet.getLastRow() - 1, HEADERS.length)
    .getValues()
    .filter(function (r) {
      return r[0];
    })
    .map(function (r) {
      const row = {};
      HEADERS.forEach(function (h, i) {
        const v = r[i];
        row[h] = v instanceof Date ? v.toISOString() : String(v);
      });
      row.images = row.images ? row.images.split(",") : [];
      if (STATUSES.indexOf(row.status) < 0) row.status = "pending";
      return row;
    });
}

function checkAdmin_(req) {
  if (!req.adminKey || req.adminKey !== prop_("ADMIN_KEY")) throw new Error("Sai mã quản trị.");
}

/** Cắt độ dài và chặn công thức: ô bắt đầu bằng = + - @ sẽ bị Google Sheet hiểu là công thức. */
function text_(value, max) {
  const s = String(value == null ? "" : value).slice(0, max).trim();
  return /^[=+\-@]/.test(s) ? "'" + s : s;
}

/** Lấy (hoặc tạo) một trang, ghi dòng tiêu đề nếu trang còn trống. Mặc định là trang "Góp ý". */
function getSheet_(name, headers) {
  name = name || SHEET_NAME;
  headers = headers || HEADERS;
  const ss = SpreadsheetApp.getActiveSpreadsheet();
  const sheet = ss.getSheetByName(name) || ss.insertSheet(name);
  if (sheet.getLastRow() === 0) {
    sheet.appendRow(headers);
    sheet.setFrozenRows(1);
  } else if (sheet.getRange(1, headers.length).getValue() !== headers[headers.length - 1]) {
    // Code mới thêm cột: ghi lại dòng tiêu đề cho đủ.
    sheet.getRange(1, 1, 1, headers.length).setValues([headers]);
  }
  return sheet;
}

function prop_(key) {
  const value = PropertiesService.getScriptProperties().getProperty(key);
  if (!value) throw new Error("Máy chủ góp ý chưa được cài đặt (chạy hàm setup).");
  return value;
}

function withLock_(fn) {
  const lock = LockService.getScriptLock();
  lock.waitLock(15000);
  try {
    return fn();
  } finally {
    lock.releaseLock();
  }
}

function json_(obj) {
  return ContentService.createTextOutput(JSON.stringify(obj)).setMimeType(ContentService.MimeType.JSON);
}
