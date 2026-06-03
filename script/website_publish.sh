cd website && npm run build
cd .. && git add docs/ && git commit -m "deploy: update website" && git push github main