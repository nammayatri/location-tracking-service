const axios = require("axios");
const fs = require("fs");

const driverUrl = "http://localhost:8016/ui";
const customerUrl = "https://api.sandbox.beckn.juspay.in/dev/app/v2";

const mobileNoUser = [
  "9999999955",
  "9999999945"
];

const mobileNoDriver = [
  "7888888881",
  "7777777771",
  "7777777772",
  "7777777774",
  "6666666668",
  "6666666662",
  "7888888771",
  "7888888772",
  "7888888773",
  "7888888774",
  "7888888775",
  "7888888776",
  "7888887776"
]

async function auth(BASE_URL, arr, fileName, merchantId) {
  const allTokens = [];
  for (let i = 0; i < arr.length; i++) {
    const authData = {
      mobileNumber: arr[i],
      mobileCountryCode: "+91",
      merchantId: merchantId,
    };


    try {
      const authRes = await axios.post(BASE_URL + "/auth", authData, {
        headers: { "Content-Type": "application/json" },
      });

      const authId = authRes.data.authId;

      const authVerifyData = {
        otp: "7891",
        deviceToken: authId,
      }; 

      const authVerifyRes = await axios.post(
        BASE_URL + `/auth/${authId}/verify`,
        authVerifyData,
        {
          headers: { "Content-Type": "application/json" },
        }
      );

      const token = authVerifyRes.data.token;
      allTokens.push(token);
    } catch (error) {
      console.error("Error while fetching API ", error);
    }
  }

  console.log(allTokens, "token");
  fs.writeFileSync(`output/${fileName}`, JSON.stringify(allTokens, null, 2));

  console.log("API response has been saved to the file:", fileName);
}

auth(customerUrl, mobileNoUser, "rider_token.json", "NAMMA_YATRI");
auth(
  driverUrl,
  mobileNoDriver,
  "driver_token.json",
  "7f7896dd-787e-4a0b-8675-e9e6fe93bb8f"
);