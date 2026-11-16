// The fixed development signing identity for `sol pack --apk` and every APK
// sol re-signs: the key and certificate of the checked-in debug keystore
// (lattice/android/app/debug.keystore, alias androiddebugkey, password
// android), exported once with openssl, so the gradle-built Player.dev and
// the runner share one signer with it. It makes a patched APK installable by
// sideloading out of the box - and by the same token, anyone holding this
// key can sign an update to any app packed with it. Never for store
// publishing; that needs a real key (okf/done/standalone-android-apk.md).
//
// Re-export after a keystore change:
//   openssl pkcs12 -in debug.keystore -passin pass:android -nocerts -nodes |
//     openssl pkcs8 -topk8 -nocrypt -outform DER | base64 -w0
//   openssl pkcs12 -in debug.keystore -passin pass:android -clcerts -nokeys |
//     openssl x509 -outform DER | base64 -w0

// PKCS#8 DER private key (RSA-2048).
export const DEV_KEY_PKCS8 = Buffer.from(
  "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCqMDJAtdQTXOQSDiQvTBXETRa5nn8PT5HQFVBtBJGeHxKFnxkx4ZYheJDc7YzfGlo9TNiRBJB1rMhDwHrlN+JZ2oBs1D42d67dbs47Kh7CAbLTpqYxkLfFJDDlUXc2TNZc8vRzm61HwoC6jP1B6EDqSMjDQbopu3aisYdPWOuLDfzSDJzYg+qIch9PAI72HYzpoZpC/aPKUaL4fpxBoJ2d8cKLb4Kst6GrsdnxUtbk8koR90VLDwfW2dFWiomjmKLCtuOKwBhi+3SUHkmLfzlVSwrBnDumkKsYWiMoowvQWwIEAA0N8qPXFUIGjuUAF1SvXWoyoAJBfaCGjak4PzFvAgMBAAECggEAFWSFjJjOlbiBeruIVOGtgsVl6+M+eYA5Bf0H5QxD+apSO7WqBxkiNAbhvsOlh43adgYo2uHF/uS/L7UMPa/upfjPV96+Cz60HmWFSu6aeyyXe/9guK5GS2OnOcwpvguejoSDsdyJxZFveQOr+hLlxg1sHw8uBXzpBr5mEdVNlmbxMysPChuI+I2smn42kKNxvoklTLKWnsg10FHz8BztfPkW0XKU/7nUkActGOxSX46POzjoD7pOF+KRQ3KNJme5WlrIjoydRtR10GavkmMz7AYS0z1oSF3V7y7o/oR9dEmnQxeHndvVr68yJvQuBRSYIN5oGVQdt8KvEx7Y2syFUQKBgQDY4Bqnu+XiGQ09G6fKlv1Px60E6VQLby0dy+XwCLnD/uDN3WBpC/Lda2nIMPi5gWip++8xzfKmvtrGmlj7HbcpjC5KjfYA078asviaHRfYbZpkEtMwemg20XBh9Nn7l0jAtI7eGgTHx70cpmYlfJNLSX+ilfALbIin67haMhDrWwKBgQDI4/QAguZnxaO+lTWOhbiu7odjomH0y0ymuCH00AavDQ46RbngkFXBhVbjch83rrYZ2L8SKfEA4MkfDtw0b7pTbDpSN9vPe0sffi/w57LR+1R0bIZuvtSqURRigH7rCE9849WJB1ZZfuDCTWEWvA3ZnteYwihhXM55Q3OLwbWyfQKBgHsMrBL/SS8/h4NkbY9Em/ToFgLzX/wc2DxPs+a9DKlns6UaNdlkLifzCdHL91NJWVkY2F4pu8NgBkEo0dOoyIOKRbmHUkyl14gOg37tgtoJVc/x3mFjEj4IFvt9MTDOZTjxOR5LJOs1KNdYkhwy0l+lFAXrjIbuzZp44+BJB9CBAoGBAJMy3LaHM/azzfC2+kBNYW7kntX1xRsO4EKFR3RP/aXExjkSoEmY2Q1s+6a7F37AiPRVamQP0WUYUKZuSBIwD/JSqkVWYr0cFYak+tHlpJTSOSb54mNBdyi5Mlue1eC3/S6A7xVyB+aZ3mzhMBPthaSBDG8FTXtR+oTGVOMB9HYtAoGBALimrM0T3w+TC6Faz/iEDRPLi4PR1jZ+WhhSOj26vwsOULlBpKD4Ydtb1+oQoDC1kXJLyj5l/h2mLAe0JXIFt/0pnEKNDtwq+JHDr+G0PIC2Lo7m1X0/Cs5PzGiY6lMx0921w/pU6t1VtpOogL5nzWqXh60qVCaRlL9qq3OgQfog",
  "base64",
)

// X.509 DER certificate for the key above (CN=Android Debug, valid to 2081).
export const DEV_CERT_DER = Buffer.from(
  "MIIDFDCCAfygAwIBAgIJAKxQ81vlXu+zMA0GCSqGSIb3DQEBDAUAMDcxCzAJBgNVBAYTAlVTMRAwDgYDVQQKEwdBbmRyb2lkMRYwFAYDVQQDEw1BbmRyb2lkIERlYnVnMCAXDTI2MDcwMTEwMDAzNVoYDzIwODEwNDAzMTAwMDM1WjA3MQswCQYDVQQGEwJVUzEQMA4GA1UEChMHQW5kcm9pZDEWMBQGA1UEAxMNQW5kcm9pZCBEZWJ1ZzCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAKowMkC11BNc5BIOJC9MFcRNFrmefw9PkdAVUG0EkZ4fEoWfGTHhliF4kNztjN8aWj1M2JEEkHWsyEPAeuU34lnagGzUPjZ3rt1uzjsqHsIBstOmpjGQt8UkMOVRdzZM1lzy9HObrUfCgLqM/UHoQOpIyMNBuim7dqKxh09Y64sN/NIMnNiD6ohyH08AjvYdjOmhmkL9o8pRovh+nEGgnZ3xwotvgqy3oaux2fFS1uTyShH3RUsPB9bZ0VaKiaOYosK244rAGGL7dJQeSYt/OVVLCsGcO6aQqxhaIyijC9BbAgQADQ3yo9cVQgaO5QAXVK9dajKgAkF9oIaNqTg/MW8CAwEAAaMhMB8wHQYDVR0OBBYEFLUO3LQdivajVVW0OBrOCIs0IfoDMA0GCSqGSIb3DQEBDAUAA4IBAQCZz+AXt9fN5RKefFqvgFj3qOOmMOf2z3F7N4N9TFDhLkjtmFN5133+H1vJdgIDL3ZG0bYwf4AsOxHSXVvQZClgWoJeUJXnaspmdDEIWgTrYY1ApG9Tkf4/wk6UuAbiwnYphItnbdGVLhOkEvy3tC9fSQHaSJPGfZ73KwizQn3y8kxKUMjpo9dfL13dquNQXIO+2XJhBsMCFDnXOgcyISrxl+R3AOtuR8XqaoV5r62VvkK39yPm9O6mKLwyWKBqArjs+0/+O0dllFqWak808tsW1BWBJsYtH4lsYspVyVzX1w16mqBTVAejZ3Opi1+H2ZXJO2LUqvrIZhJ80uTcTOZM",
  "base64",
)
