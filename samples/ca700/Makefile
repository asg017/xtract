sample.pdf:
	curl -o $@ https://wcfweenxfcmsichcbyki.supabase.in/storage/v1/object/public/pdfs/487cd987-6db0-418e-a60f-42984bb28db4.pdf

tmp.db: sample.pdf
	xtract extract form700.md sample.pdf --screenshot -o $@