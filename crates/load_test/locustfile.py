from locust import HttpUser, task, between
import random
import datetime
import random
import json


class LocationUpdate(HttpUser):
    def on_start(self):
        self.token = [
        "89865ef5-da51-4c1b-b383-862e57504a0e",
        "75ba0a82-f750-4539-b067-d5c79fba4dad",
        "7c722062-95a7-40f2-8edd-01e71b1c4122",
        "c5c668f9-f127-4a86-b115-594e52b6829a",
        "acbcdde0-ec76-4593-bf25-86c58638ea8d",
        "1d653a3b-bde3-4fdd-96ff-a748c9e210bd",
        "f9270164-5e20-4c44-b563-d062a02c80df",
        "7009c594-1c5c-4d22-8071-0fdea8fd2b75",
        "7f160752-8070-4fca-adc1-4735069b6812",
        "576d20d6-896f-4719-ac7f-8dd5fe1e606b",
        "e583da16-d331-4bd3-b8d6-a6f3bc5bb8bf",
        "122bf13f-e873-40fa-84d1-c0a4cac1f854",
        "77f05c66-5712-4e06-9bce-56420ae69557"
        ]
        self.mId = "favorit0-0000-0000-0000-00000favorit"
        self.vt = "SUV"
    @task
    def location_updates(self):


        current_utc_time = datetime.datetime.utcnow()

        formatted_utc_time = current_utc_time.strftime('%Y-%m-%dT%H:%M:%SZ')
        
        token = random.choice(self.token)
        print(token)

        headers = {
            "Content-Type": "application/json",
            "token": token,
            "mId": self.mId,
            "vt": self.vt
        }

        locationData =[{
            "pt": {
                "lat": random.uniform(13.0, 16.5), 
                "lon": random.uniform(75.0, 76.5)
             },
            "ts": "2023-08-29T08:34:27Z",
   	        "acc": 1
        }]
        
        print(locationData, "location data")
        
        with self.client.post(url = "/driver/location", json=locationData, headers=headers) as response:
            print(response.json(), "location results")


class RideSearch(HttpUser):
    wait_time = between(1,2)
    
    def on_start(self):
        self.token = "c892e9cb-0a02-4966-8f2b-8a27bcbd972f" 

    @task
    def ride_search(self):
        ride_data = {
            "fareProductType": "ONE_WAY",
            "contents": {
                "origin": {
                    "address": {
                        "area": "8th Block Koramangala",
                        "areaCode": "560047",
                        "building": "Juspay Buildings",
                        "city": "Bangalore",
                        "country": "India",
                        "door": "#444",
                        "street": "18th Main",
                        "state": "Karnataka"
                    },
                    "gps": {
                        "lat": 16.479615647756166,
                        "lon": 75.39032459232
                    }
                },
                "destination": {
                    "address": {
                        "area": "6th Block Koramangala",
                        "areaCode": "560047",
                        "building": "Juspay Apartments",
                        "city": "Bangalore",
                        "country": "India",
                        "door": "#444",
                        "street": "18th Main",
                        "state": "Karnataka"
                    },
                    "gps": {
                        "lat": 16.42990908333854,
                        "lon": 75.36080308508846
                    }
                }
            }
        }

        headers = {
            "Content-Type": "application/json",
            "token": self.token,
        }

        with self.client.post(url = "/rideSearch", json=ride_data, headers=headers) as response:
            print(response.json(), "ride search")

class RideSearchResults(HttpUser):
    wait_time = between(1,2)

    def on_start(self):
        self.token = "00ade58d-c526-43f5-9ce4-1f538388b526"  
        self.searchId = "7d6c2b0a-d6f8-4913-86fb-7202c48e2a99"

    @task
    def ride_search_results(self):

        headers = {
            "Content-Type": "application/json",
            "token": self.token,
        }

        with self.client.get(url = "/rideSearch/{searchId}/results".format(searchId = self.searchId), headers=headers) as response:

            # search_id = response.json().get("searchId", None)
            print(response.json(), "ride search results")


class EstimateSelect(HttpUser):
    wait_time = between(1,2)

    def on_start(self):
        self.token = "00ade58d-c526-43f5-9ce4-1f538388b526"  
        self.estimateId = "69f585bd-7f58-44a5-ac98-ae9301822e0d"
    
    @task
    def estimate_select(self):
        headers = {
            "Content-Type": "application/json",
            "token": self.token,
        }

        estimateData =  {
            "customerExtraFee": None,
            "autoAssignEnabledV2": True,
            "autoAssignEnabled": True
        }

        with self.client.post(url = "/estimate/{estimateId}/select2".format(estimateId = self.estimateId), json=estimateData, headers=headers) as response:
            print(response.json(), "estimate select results")

class QuoteConfirm(HttpUser):
    wait_time = between(1,2)

    def on_start(self):
        self.token = "00ade58d-c526-43f5-9ce4-1f538388b526"  
        self.searchReqId = "bee5eb3f-2be8-4936-ac82-ced05b2bfe55"
    
    @task
    def quote_confirm(self):
        headers = {
            "Content-Type": "application/json",
            "token": self.token,
        }

        quoteData =  {
                "searchRequestId": self.searchReqId,
                "offeredFare" : 10
            }
        
        with self.client.post(url = "/driver/searchRequest/quote/offer", json=quoteData, headers=headers) as response:
            print(response.json(), "quote results")


class StartRide(HttpUser):
    wait_time = between(1,2)

    def on_start(self):
        self.token = "00ade58d-c526-43f5-9ce4-1f538388b526"  
        self.rideId = "bee5eb3f-2be8-4936-ac82-ced05b2bfe55"
        self.rideOtp = ""
    
    @task
    def start_ride(self):
        headers = {
            "Content-Type": "application/json",
            "token": self.token,
        }

        rideData = {
            "rideOtp": self.rideOtp,
            "point": {
                "lat": "12.9783131063211",
                "lon": "77.595122819161"
             }
        }
        
        with self.client.post(url = "/driver/ride/{rideId}/start".format(rideId = self.rideId), json=rideData, headers=headers) as response:
            print(response.json(), "quote results")