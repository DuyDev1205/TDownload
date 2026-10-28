const PREFIX = "vidsaver.";

export function load(key: string, fallback: string): string {
  try {
    return localStorage.getItem(PREFIX + key) ?? fallback;
  } catch {
    return fallback;
  }
}

export function save(key: string, value: string): void {
  try {
    localStorage.setItem(PREFIX + key, value);
  } catch {
    // Không lưu được cài đặt thì vẫn dùng bình thường trong phiên này.
  }
}
