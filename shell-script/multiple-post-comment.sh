#!/bin/bash

for i in {0..100}
do
  curl -X POST -H "Content-Type: application/json" -d '{"utc" : "2014-10-10T04:50:40Z" , "name" : "test" , "text" : "hello"}' --insecure https://localhost:3000/create
done