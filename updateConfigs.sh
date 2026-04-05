#!/bin/sh
cd "/home/diver/sources/JS/из NSQCuE/global-mouse-hook/"
j=$(date)
git add .
git commit -m "$1 $j"
git push git@github.com:Vladgobelen/global-mouse-hook.git
git add .
git commit -m "$1 $j"
git push

