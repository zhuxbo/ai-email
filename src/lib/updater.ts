export type UpdatePlatform = 'android' | 'macos' | 'unsupported';

export function detectUpdatePlatform(userAgent = navigator.userAgent): UpdatePlatform {
  if (/Android/i.test(userAgent)) {
    return 'android';
  }
  if (/Macintosh|Mac OS X/i.test(userAgent)) {
    return 'macos';
  }
  return 'unsupported';
}
