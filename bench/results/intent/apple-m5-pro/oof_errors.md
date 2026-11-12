## qwen3.5-2b rules_kw_fewshot_echo: 137 of 1679 wrong out-of-fold (91.8 accurate)

| gold | predicted | cases |
|---|---|---|
| rewrite | polish | 17 |
| summarize | rewrite | 13 |
| polish | rewrite | 13 |
| rewrite | summarize | 10 |
| rewrite | reply | 8 |
| reply | summarize | 6 |
| cancel | reply | 5 |
| cancel | rewrite | 5 |
| polish | summarize | 4 |
| translate | summarize | 4 |
| summarize | polish | 4 |
| reply | none | 4 |
| reply | rewrite | 4 |
| none | cancel | 3 |
| reply | polish | 3 |
| summarize | translate | 3 |
| summarize | reply | 3 |
| summarize | none | 2 |
| cancel | translate | 2 |
| reply | translate | 2 |
| translate | rewrite | 2 |
| polish | reply | 2 |
| rewrite | translate | 2 |
| none | reply | 1 |
| ask | summarize | 1 |
| translate | reply | 1 |
| rewrite | none | 1 |
| cancel | none | 1 |
| cancel | summarize | 1 |
| none | translate | 1 |
| polish | cancel | 1 |
| polish | translate | 1 |
| reply | cancel | 1 |
| reply | ask | 1 |
| ask | polish | 1 |
| rewrite | cancel | 1 |
| cancel | polish | 1 |
| none | summarize | 1 |
| summarize | ask | 1 |

| tag | cases | wrong | wrong share |
|---|---|---|---|
| asr_error | 542 | 85 | 15.7 |
| self_correction | 181 | 18 | 9.9 |
| retraction | 97 | 8 | 8.2 |
| negation | 116 | 13 | 11.2 |
| filler | 486 | 30 | 6.2 |
| code_mix | 96 | 5 | 5.2 |
| polite | 149 | 12 | 8.1 |
| multi_request | 274 | 29 | 10.6 |
| question | 240 | 11 | 4.6 |

| id | gold | predicted | p(gold) | p(top) | tags | text |
|---|---|---|---|---|---|---|
| ja-tc077 | ask | polish | 0.04 | 0.84 | question, asr_error | この感じなんて読むの |
| en-tc069 | ask | summarize | 0.22 | 0.46 | question, asr_error | who scent this memo |
| ja-ta048 | cancel | none | 0.07 | 0.55 | asr_error, filler | あ、えっと、とりけして |
| zh-tb105 | cancel | polish | 0.37 | 0.39 | asr_error | 布用了 |
| en-ta097 | cancel | reply | 0.06 | 0.49 | asr_error, filler | uh for get it |
| en-tb113 | cancel | reply | 0.02 | 0.40 | asr_error | for get it |
| en-tb121 | cancel | reply | 0.34 | 0.65 | asr_error, retraction | can sell that i changed my mind |
| en-td092 | cancel | reply | 0.41 | 0.52 |  | not right now |
| zh-tc147 | cancel | reply | 0.45 | 0.55 | retraction, filler, asr_error | 那个帮我回复一下，嗯想了想还是蒜了吧 |
| en-ta102 | cancel | rewrite | 0.42 | 0.58 | asr_error, self_correction, retraction | re write it never mine |
| en-td091 | cancel | rewrite | 0.10 | 0.87 | retraction | expand this, no forget it |
| ja-ta040 | cancel | rewrite | 0.37 | 0.41 | self_correction, retraction | 箇条書きにして…今のなしで |
| zh-ta134 | cancel | rewrite | 0.44 | 0.47 |  | 算了 |
| zh-tb069 | cancel | rewrite | 0.03 | 0.96 | asr_error, retraction | 帮我扩写……布要了，曲消 |
| ja-ta056 | cancel | summarize | 0.02 | 0.72 | asr_error | ストップ、注視して |
| en-tb117 | cancel | translate | 0.11 | 0.75 | retraction, self_correction | translate this into japanese cancel that |
| en-td087 | cancel | translate | 0.47 | 0.51 | retraction | put this in English, nah |
| en-tb137 | none | cancel | 0.10 | 0.78 | self_correction | rewrite this no actually just delete the whole paragraph |
| zh-tb072 | none | cancel | 0.22 | 0.74 | filler | 嗯，把这段删掉 |
| zh-td094 | none | cancel | 0.00 | 0.99 |  | 帮我把这段删了吧 |
| en-ta149 | none | reply | 0.48 | 0.50 |  | hi sarah thanks so much for sending over the report i'll look at it tomorrow morning and get back to you with any questions before the end of the day |
| zh-tc156 | none | summarize | 0.00 | 0.97 | asr_error | 把群里这条通知制订一下 |
| ja-ta103 | none | translate | 0.42 | 0.42 | code_mix | thank you、助かりました |
| ja-ta147 | polish | cancel | 0.12 | 0.74 | asr_error, self_correction, polite | 要約…あ、いや、水耕お願いします |
| ja-td057 | polish | reply | 0.02 | 0.96 | asr_error | さっきの以来の件で部長への返事なんだけど、変な日本語ないか見て |
| zh-td056 | polish | reply | 0.41 | 0.55 | filler | 那个我在手机上打字太快了手指头老按偏，这是我要回他的，你帮我改改错字别让人家笑话我 |
| en-ta068 | polish | rewrite | 0.20 | 0.71 | asr_error, polite, question | can you pole ish this for me |
| en-ta093 | polish | rewrite | 0.22 | 0.73 | asr_error, negation | no translation needed just make this red more naturally |
| en-tb009 | polish | rewrite | 0.41 | 0.55 | asr_error | make this read more naturally the commas are all over the place and a couple of words got miss heard because i was talking really fast into the mic while walking to class |
| ja-ta122 | polish | rewrite | 0.08 | 0.91 | asr_error, negation | 書き換えはしないで、文章をととのえて |
| ja-tb018 | polish | rewrite | 0.09 | 0.47 | asr_error | このゼミのレジュメ、ですます調を揃えて、あと五時もよろしく |
| ja-tc125 | polish | rewrite | 0.37 | 0.61 | filler | なんか引っかかって読みにくいんだよね、意味変えずにすっと読めるようにして |
| zh-ta014 | polish | rewrite | 0.01 | 0.98 | asr_error | 闰色一下 |
| zh-ta032 | polish | rewrite | 0.44 | 0.53 | filler | 嗯，这是我发给客户的邮件草稿，你帮我检查一下语法和用词，改得通顺自然一点，但是不要变长 |
| zh-ta095 | polish | rewrite | 0.28 | 0.71 | filler | 那个，你帮我把这段话弄得顺口一点，不要改意思，因为是要发给老师的，用词稍微注意一点 |
| zh-ta147 | polish | rewrite | 0.23 | 0.72 | question, polite | 能不能麻烦你把这条消息理顺一点？读起来有点别扭，我是要发给部门群的，不想让大家觉得我写得太乱 |
| zh-tb015 | polish | rewrite | 0.03 | 0.96 | asr_error | 帮我润色润色，我这篇意论文写得有点像流水账，不过字数别增减太多，老师说不能超过一千二，所以别写太长 |
| zh-tb019 | polish | rewrite | 0.16 | 0.79 | asr_error, filler | 把它理顺一点，呃，对，就是并句 |
| zh-tc131 | polish | rewrite | 0.33 | 0.47 | asr_error | 帮我润泽一下这句 |
| en-ta044 | polish | summarize | 0.01 | 0.97 | asr_error, filler | uh just tie D this up a little it's kind of messy |
| ja-ta034 | polish | summarize | 0.03 | 0.92 | asr_error, negation | 要約はいいから、こうせいして |
| ja-tc131 | polish | summarize | 0.34 | 0.65 | code_mix, asr_error | SNSに出すこの投稿のうち間違い直して |
| ja-tc132 | polish | summarize | 0.06 | 0.37 | asr_error | これ構成して |
| ja-tb005 | polish | translate | 0.36 | 0.60 | self_correction | 訳して、やっぱり句読点をいい感じに打つだけでいい |
| ja-tc040 | reply | ask | 0.20 | 0.62 | asr_error | 当日何時に付くか聞いて |
| ja-tc007 | reply | cancel | 0.02 | 0.62 | self_correction, asr_error | これどういう意味、あいやいいや、とりあえず検診書いといて |
| en-tc018 | reply | none | 0.21 | 0.58 | asr_error | rely got it I'm setting the staging server up right now |
| en-tc048 | reply | none | 0.10 | 0.88 | filler | uh thank Professor Miller for all the detailed feedback on my draft |
| en-td030 | reply | none | 0.47 | 0.49 |  | thank Dr. Patel for all the advice |
| zh-tc008 | reply | none | 0.20 | 0.51 | asr_error | 帮我汇付一下底下这留言 |
| en-tc004 | reply | polish | 0.13 | 0.86 | filler, multi_request, asr_error | drop a quick comment back on this post and uh proof reed it for typos before I hit send |
| ja-tc005 | reply | polish | 0.40 | 0.47 | filler | この、このメールの返事ちょっと考えて |
| ja-tc024 | reply | polish | 0.05 | 0.93 | asr_error | いいですって変えしといて |
| ja-tc001 | reply | rewrite | 0.05 | 0.47 | filler, asr_error | えーと、これ変身しといて |
| ja-tc051 | reply | rewrite | 0.34 | 0.65 | filler | あの、お礼の返事なんだけど、あんまりかしこまりすぎない感じで書いて |
| ja-td069 | reply | rewrite | 0.15 | 0.84 |  | もっと明るい感じで返信して |
| zh-tc033 | reply | rewrite | 0.28 | 0.38 | multi_request, asr_error | 帮我用英文把这个商务合作邀约给锯掉，写得礼貌点 |
| en-tc012 | reply | summarize | 0.09 | 0.46 | asr_error | right back to this Slack message |
| en-tc015 | reply | summarize | 0.27 | 0.61 | self_correction | sum this up for me, oh wait no it's just scheduling, tell him sure Wednesday afternoon works fine |
| en-tc025 | reply | summarize | 0.11 | 0.80 | asr_error | torn down this dinner invite nicely |
| zh-tc012 | reply | summarize | 0.37 | 0.50 | filler, asr_error | 那个帮我会一下这句 |
| zh-tc025 | reply | summarize | 0.01 | 0.91 | asr_error | 帮我玩具一下今晚这饭局 |
| zh-td023 | reply | summarize | 0.19 | 0.80 | filler, multi_request | 哎老板大清早甩了这么长一串小作文过来我实在没空细看，你先总结一下他说了啥，再帮我回一句 |
| en-td024 | reply | translate | 0.05 | 0.95 | filler, multi_request | um write a reply to this and then translate it into French |
| zh-td024 | reply | translate | 0.06 | 0.94 | multi_request | 这条你先帮我回一下，再翻成英文 |
| zh-ta057 | rewrite | cancel | 0.16 | 0.44 | asr_error | 改的正是一点 |
| ja-ta009 | rewrite | none | 0.03 | 0.62 | asr_error, filler | えーと、かひつして |
| en-ta104 | rewrite | polish | 0.41 | 0.56 | filler, multi_request | um make it friendlier and uh fix the typos |
| en-tb070 | rewrite | polish | 0.04 | 0.90 | asr_error, negation | do not translate it just rewrite this for be gin errs |
| en-td006 | rewrite | polish | 0.25 | 0.69 | asr_error, filler, multi_request | um I put this peace together for our team newsletter earlier today, add a bit more to it and then clean the whole thing up for me |
| en-td008 | rewrite | polish | 0.38 | 0.56 | filler, multi_request | uh this is just for our team chat so fix the punctuation and make it more relaxed |
| ja-ta027 | rewrite | polish | 0.02 | 0.90 | asr_error, question, polite | 二区付けしてもらえますか |
| ja-ta088 | rewrite | polish | 0.38 | 0.57 | self_correction | 最初は校正だけって言ったけど、やっぱり内容を整理して、読みやすい箇条書きにして |
| ja-tb034 | rewrite | polish | 0.43 | 0.49 | multi_request | 肉付けして、誤字もついでに直して |
| ja-tb046 | rewrite | polish | 0.20 | 0.59 | asr_error | 警護にして |
| ja-tb059 | rewrite | polish | 0.31 | 0.42 | asr_error, polite, question | このドキュメント、プログラミング初めての人が読むので、所新者向けに書き直してもらえますか |
| ja-tb064 | rewrite | polish | 0.07 | 0.90 | asr_error, question | マーク ダウンの表にしてくれる？ |
| ja-tc098 | rewrite | polish | 0.40 | 0.42 | asr_error | この返信五十時以内に縮めて |
| zh-ta084 | rewrite | polish | 0.00 | 0.99 | asr_error, negation | 别翻译，帮我改些一下 |
| zh-tb032 | rewrite | polish | 0.10 | 0.66 | asr_error, negation | 帮我括写一下这句话，别缩短 |
| zh-tb134 | rewrite | polish | 0.19 | 0.78 | multi_request | 把这句话换种方式表达，意思不变，然后顺一顺语句 |
| zh-tc099 | rewrite | polish | 0.11 | 0.84 | filler, self_correction | 帮我回他一下，啊不对不对这段就是我自己写好要发出去的，你帮我把语气改得客气庄重一点就行 |
| zh-td004 | rewrite | polish | 0.43 | 0.44 | filler, multi_request | 那个这是我刚在备忘录里瞎敲的一段自我介绍，下周要给院里领导看的，你先帮我顺一下语病，再改得正式一点 |
| zh-td007 | rewrite | polish | 0.39 | 0.53 | filler, code_mix, multi_request | 嗯这段我要贴到 PPT 里的，帮我改成列表，顺便把标点理一下 |
| en-tb046 | rewrite | reply | 0.43 | 0.49 | question, polite, asr_error | can you make this more polite i sound kind of rude in this email to my land lord but honestly i'm just stressed about the broken heater and want to stay on good terms |
| en-tb057 | rewrite | reply | 0.33 | 0.63 |  | make this an email to my professor asking for an extension |
| en-tb064 | rewrite | reply | 0.10 | 0.25 | asr_error | reward this sentence |
| ja-tc089 | rewrite | reply | 0.46 | 0.53 |  | このメモ、ちゃんとした返信メールの形にして |
| zh-ta028 | rewrite | reply | 0.07 | 0.90 | asr_error | 口语话一点 |
| zh-ta043 | rewrite | reply | 0.24 | 0.48 | asr_error | 帮我瑞来特一下这句话 |
| zh-tb089 | rewrite | reply | 0.15 | 0.44 | self_correction | 帮我把这段扩写一下，啊不对，是砍掉一半，意思留着 |
| zh-tc089 | rewrite | reply | 0.26 | 0.74 |  | 把这段帮我改成回信的格式 |
| en-ta064 | rewrite | summarize | 0.05 | 0.59 | asr_error, polite, question | can you a labor rate on this point |
| en-ta105 | rewrite | summarize | 0.38 | 0.44 | asr_error, self_correction, filler | summarize it uh no wait expense it instead |
| en-tb035 | rewrite | summarize | 0.15 | 0.84 | self_correction, asr_error | summarize this no actually ad more detail instead |
| en-tc101 | rewrite | summarize | 0.25 | 0.73 |  | take these scattered Slack blurbs and combine them into one clean email |
| ja-ta023 | rewrite | summarize | 0.36 | 0.47 | polite | メールが長すぎて読んでもらえなさそうだから、大事なところだけ残して、半分くらいの長さに削ってほしいです |
| ja-ta140 | rewrite | summarize | 0.14 | 0.84 | negation | 要約しないで、箇条書きにして |
| ja-ta149 | rewrite | summarize | 0.16 | 0.81 | asr_error, negation | 翻訳はいらないから、単縮して |
| zh-ta150 | rewrite | summarize | 0.03 | 0.90 | asr_error, self_correction | 帮我总结一下……不对，经简一下，保留原来的意思，不用改语气，只要把废话去掉就行，谢谢了 |
| zh-tb129 | rewrite | summarize | 0.01 | 0.97 | negation | 不用总结，给它起个标题，十个字左右 |
| zh-tb149 | rewrite | summarize | 0.21 | 0.76 | asr_error | 这页批批踢字太多了，帮我经简一下，只留最关键的几个词 |
| zh-tb059 | rewrite | translate | 0.38 | 0.53 | self_correction | 帮我翻译一下，不是，改得口语化一点，我要发到评论区 |
| zh-tb077 | rewrite | translate | 0.23 | 0.63 | self_correction, filler | 欸，帮我翻译成日文，哦不，换成第一人称来写就行 |
| zh-td076 | summarize | ask | 0.12 | 0.70 | asr_error, question | 福建里这份文档主要内容是什么 |
| en-ta075 | summarize | none | 0.02 | 0.77 | asr_error, code_mix | summer eyes this bitte |
| ja-ta076 | summarize | none | 0.39 | 0.44 | asr_error, filler | えっと、ようやくして |
| en-tb092 | summarize | polish | 0.01 | 0.74 | asr_error, multi_request | summer eyes this and clean it up i've got an exam tomorrow |
| ja-ta022 | summarize | polish | 0.12 | 0.77 | asr_error, negation, polite | 翻訳じゃなくて、用紙をまとめてください |
| zh-ta030 | summarize | polish | 0.01 | 0.99 | negation, asr_error | 不用翻译了，直接总洁一下 |
| zh-tb022 | summarize | polish | 0.03 | 0.97 | asr_error, multi_request | 体炼一下重点，我要放到笔记里，对了错别字也顺手改改，这是上午讲座的录音转写，有点乱，晚上再整理 |
| ja-tc116 | summarize | reply | 0.04 | 0.88 | negation, asr_error | まだ返信はいいからさ、とりあえず漸くだけして |
| zh-tb057 | summarize | reply | 0.20 | 0.51 | asr_error, negation | 不用翻译，也不用润色，帮我写个债要 |
| zh-tc110 | summarize | reply | 0.00 | 0.80 | asr_error | 帮我终结一下这条群公告 |
| en-ta057 | summarize | rewrite | 0.40 | 0.41 | filler, multi_request | uh i need to forward this report to the director so summarize the main findings for me and tidy up the wording a bit while you're at it |
| en-tb101 | summarize | rewrite | 0.23 | 0.68 | multi_request, asr_error | some arise this then make it more formal |
| ja-ta024 | summarize | rewrite | 0.36 | 0.64 | multi_request | 三行にまとめて、ちょっとカジュアルな言い方にして |
| ja-ta110 | summarize | rewrite | 0.23 | 0.59 | asr_error, multi_request, polite | まず誤字を直して、そのあと文章全体のようてんを、できるだけ短くまとめてください |
| ja-tb104 | summarize | rewrite | 0.14 | 0.49 | filler, polite, multi_request | この記事、要約して、あと敬語にして、ええと、先生にメールで送れるように、お願いします |
| ja-tb107 | summarize | rewrite | 0.45 | 0.53 | multi_request | まず文章を整えて、そのあと動画の説明欄に使うから一行でまとめて |
| ja-tb108 | summarize | rewrite | 0.07 | 0.93 | multi_request | ポイントを抜き出して、もうちょっと砕けた口調で |
| ja-tc119 | summarize | rewrite | 0.02 | 0.79 | asr_error | これ短く予約して |
| ja-td010 | summarize | rewrite | 0.06 | 0.92 | asr_error, multi_request | 車内向けのやつだから丁寧にしてから三つにまとめて |
| ja-td014 | summarize | rewrite | 0.44 | 0.53 | multi_request | 後輩に見せるやつだからざっくりまとめて、話し言葉っぽく |
| zh-ta039 | summarize | rewrite | 0.03 | 0.97 | asr_error, multi_request | 总节一下，再改得口语化一点 |
| zh-tb036 | summarize | rewrite | 0.02 | 0.97 | code_mix | 一句话描述一下这个 PR，我要贴到 description 里，别写太长，让评审的同学一眼就能看明白改了什么 |
| zh-tb143 | summarize | rewrite | 0.08 | 0.91 | asr_error, self_correction | 帮我润色……不对，用一句话概扩就行 |
| ja-ta001 | summarize | translate | 0.15 | 0.79 | asr_error, multi_request | 箇条書きにして、それをヨーヤクして |
| ja-ta143 | summarize | translate | 0.03 | 0.58 | asr_error, self_correction | 翻訳して…いや、要訳して |
| ja-tb096 | summarize | translate | 0.00 | 1.00 | asr_error | このスライドの説明、長いから要訳して |
| en-tc134 | translate | reply | 0.06 | 0.91 | asr_error | trains late my reply into Spanish |
| ja-tb083 | translate | rewrite | 0.23 | 0.64 | asr_error, polite, multi_request, question | 誤字を直して、漢国語にしてもらえますか |
| ja-tc138 | translate | rewrite | 0.03 | 0.95 | asr_error | 台湾のお客さん向けだから反対字にして |
| en-ta058 | translate | summarize | 0.35 | 0.62 | multi_request | so i got this long update from the vendor and i need it in spanish for the warehouse team and then after that can you also summarize it |
| ja-ta050 | translate | summarize | 0.01 | 0.75 | asr_error | わやくして |
| ja-ta126 | translate | summarize | 0.01 | 0.99 | asr_error, multi_request | 英語にして、それから三行で用約して |
| ja-tb091 | translate | summarize | 0.40 | 0.60 | asr_error, multi_request | 英語に翻訳して、それから用点をまとめて |

## qwen3.5-4b rules_kw_fewshot_echo: 91 of 1679 wrong out-of-fold (94.6 accurate)

| gold | predicted | cases |
|---|---|---|
| polish | rewrite | 10 |
| summarize | rewrite | 10 |
| rewrite | polish | 9 |
| rewrite | summarize | 6 |
| reply | none | 5 |
| translate | summarize | 4 |
| rewrite | reply | 3 |
| reply | ask | 3 |
| summarize | ask | 3 |
| rewrite | translate | 3 |
| cancel | reply | 3 |
| cancel | rewrite | 2 |
| rewrite | cancel | 2 |
| cancel | none | 2 |
| reply | translate | 2 |
| none | reply | 2 |
| summarize | translate | 2 |
| reply | summarize | 2 |
| polish | none | 1 |
| polish | cancel | 1 |
| rewrite | ask | 1 |
| none | rewrite | 1 |
| translate | reply | 1 |
| polish | translate | 1 |
| reply | rewrite | 1 |
| ask | none | 1 |
| ask | translate | 1 |
| summarize | reply | 1 |
| polish | reply | 1 |
| reply | polish | 1 |
| translate | rewrite | 1 |
| ask | polish | 1 |
| polish | summarize | 1 |
| summarize | none | 1 |
| none | polish | 1 |
| summarize | polish | 1 |

| tag | cases | wrong | wrong share |
|---|---|---|---|
| asr_error | 542 | 57 | 10.5 |
| self_correction | 181 | 21 | 11.6 |
| retraction | 97 | 3 | 3.1 |
| negation | 116 | 6 | 5.2 |
| filler | 486 | 22 | 4.5 |
| code_mix | 96 | 4 | 4.2 |
| polite | 149 | 4 | 2.7 |
| multi_request | 274 | 18 | 6.6 |
| question | 240 | 9 | 3.8 |

| id | gold | predicted | p(gold) | p(top) | tags | text |
|---|---|---|---|---|---|---|
| ja-tc078 | ask | none | 0.49 | 0.50 | filler, question, code_mix | なんか怪しいSMSきたんだけど、このリンク開いて大丈夫かな |
| zh-tb016 | ask | polish | 0.29 | 0.35 | question | 你是谁啊 |
| ja-tc082 | ask | translate | 0.21 | 0.64 | self_correction | これ訳して、あ違うわ、そもそもこれ何語なのかだけ教えて |
| en-tb113 | cancel | none | 0.12 | 0.54 | asr_error | for get it |
| ja-ta056 | cancel | none | 0.00 | 0.86 | asr_error | ストップ、注視して |
| ja-ta048 | cancel | reply | 0.13 | 0.24 | asr_error, filler | あ、えっと、とりけして |
| ja-td085 | cancel | reply | 0.36 | 0.43 |  | いやなんでもない |
| zh-tc147 | cancel | reply | 0.28 | 0.70 | retraction, filler, asr_error | 那个帮我回复一下，嗯想了想还是蒜了吧 |
| en-ta102 | cancel | rewrite | 0.12 | 0.75 | asr_error, self_correction, retraction | re write it never mine |
| en-ta148 | cancel | rewrite | 0.10 | 0.88 | asr_error, self_correction, retraction | make it shorter oh no wait council that |
| zh-tc156 | none | polish | 0.00 | 0.55 | asr_error | 把群里这条通知制订一下 |
| en-tc152 | none | reply | 0.42 | 0.43 | filler, asr_error | uh four word this email thread over to Liam in accounting |
| ja-tb144 | none | reply | 0.30 | 0.59 | asr_error | レポートの提出機嫌は金曜日の十七時です |
| en-tb137 | none | rewrite | 0.27 | 0.45 | self_correction | rewrite this no actually just delete the whole paragraph |
| en-ta052 | polish | cancel | 0.35 | 0.37 | self_correction | summarize it no no polish it |
| en-ta028 | polish | none | 0.18 | 0.81 | self_correction, filler | translate it uh actually never mind just check the spelling |
| ja-tc122 | polish | reply | 0.17 | 0.81 | asr_error | この下書きの返信、５時ないか見て直して |
| en-ta093 | polish | rewrite | 0.30 | 0.68 | asr_error, negation | no translation needed just make this red more naturally |
| en-tb025 | polish | rewrite | 0.24 | 0.59 | asr_error | tidy cup this paragraph |
| ja-ta068 | polish | rewrite | 0.21 | 0.27 | asr_error, polite | 校庭お願いします |
| ja-ta107 | polish | rewrite | 0.19 | 0.53 | asr_error, self_correction, negation, question, polite | 翻訳…じゃなくて、聖書してくれる？ |
| ja-tb018 | polish | rewrite | 0.12 | 0.37 | asr_error | このゼミのレジュメ、ですます調を揃えて、あと五時もよろしく |
| ja-tc132 | polish | rewrite | 0.15 | 0.77 | asr_error | これ構成して |
| zh-ta095 | polish | rewrite | 0.29 | 0.68 | filler | 那个，你帮我把这段话弄得顺口一点，不要改意思，因为是要发给老师的，用词稍微注意一点 |
| zh-tb015 | polish | rewrite | 0.21 | 0.76 | asr_error | 帮我润色润色，我这篇意论文写得有点像流水账，不过字数别增减太多，老师说不能超过一千二，所以别写太长 |
| zh-tb123 | polish | rewrite | 0.09 | 0.90 | self_correction | 请把这段改得正式一点……不好意思说错了，是把错别字和病句改正过来 |
| zh-tb138 | polish | rewrite | 0.05 | 0.74 | asr_error | 帮我改错别字，然后给我加上表点 |
| zh-tb107 | polish | summarize | 0.20 | 0.60 | self_correction | 帮我总结一下，啊不对，润色就行 |
| ja-tb005 | polish | translate | 0.19 | 0.50 | self_correction | 訳して、やっぱり句読点をいい感じに打つだけでいい |
| en-tc039 | reply | ask | 0.34 | 0.62 | filler | uh ask him which file he's talking about here |
| ja-tc040 | reply | ask | 0.10 | 0.89 | asr_error | 当日何時に付くか聞いて |
| zh-tc011 | reply | ask | 0.36 | 0.63 | filler, question | 嗯像主管在群里发的这种话要不要回啊，要回的话该怎么说 |
| en-tc018 | reply | none | 0.00 | 1.00 | asr_error | rely got it I'm setting the staging server up right now |
| ja-tc012 | reply | none | 0.18 | 0.64 | asr_error | これ帰しといて |
| zh-tc008 | reply | none | 0.03 | 0.68 | asr_error | 帮我汇付一下底下这留言 |
| zh-tc024 | reply | none | 0.01 | 0.97 | asr_error | 直接帮我汇个可以过去 |
| zh-td034 | reply | none | 0.08 | 0.90 | filler | 我今儿发烧起不来床了，嗯说我今天来不了 |
| ja-td033 | reply | polish | 0.15 | 0.32 |  | 了解ですとだけね |
| ja-tc001 | reply | rewrite | 0.19 | 0.41 | filler, asr_error | えーと、これ変身しといて |
| ja-tc039 | reply | summarize | 0.09 | 0.55 | filler, asr_error | えっと、結局どのファイルの話なのか危機返しといて |
| zh-td023 | reply | summarize | 0.42 | 0.58 | filler, multi_request | 哎老板大清早甩了这么长一串小作文过来我实在没空细看，你先总结一下他说了啥，再帮我回一句 |
| en-tc033 | reply | translate | 0.00 | 1.00 | multi_request, asr_error | the climb this sponsorship pitch in Spanish |
| zh-tc033 | reply | translate | 0.23 | 0.23 | multi_request, asr_error | 帮我用英文把这个商务合作邀约给锯掉，写得礼貌点 |
| en-ta064 | rewrite | ask | 0.08 | 0.39 | asr_error, polite, question | can you a labor rate on this point |
| en-tb040 | rewrite | cancel | 0.36 | 0.60 | self_correction | make it longer no wait scratch that make it shorter |
| ja-ta009 | rewrite | cancel | 0.08 | 0.43 | asr_error, filler | えーと、かひつして |
| en-ta105 | rewrite | polish | 0.06 | 0.47 | asr_error, self_correction, filler | summarize it uh no wait expense it instead |
| en-ta107 | rewrite | polish | 0.37 | 0.42 | asr_error, self_correction | fix the grammar actually no turn it into an e mail |
| en-tb070 | rewrite | polish | 0.34 | 0.64 | asr_error, negation | do not translate it just rewrite this for be gin errs |
| en-td008 | rewrite | polish | 0.43 | 0.48 | filler, multi_request | uh this is just for our team chat so fix the punctuation and make it more relaxed |
| ja-ta027 | rewrite | polish | 0.01 | 0.96 | asr_error, question, polite | 二区付けしてもらえますか |
| ja-td008 | rewrite | polish | 0.01 | 0.99 | multi_request | ここ句読点直して、あともっとくだけた感じにして |
| zh-ta043 | rewrite | polish | 0.00 | 0.92 | asr_error | 帮我瑞来特一下这句话 |
| zh-ta084 | rewrite | polish | 0.00 | 0.99 | asr_error, negation | 别翻译，帮我改些一下 |
| zh-ta130 | rewrite | polish | 0.31 | 0.55 | code_mix | 帮我 rewrite 一下 |
| en-ta032 | rewrite | reply | 0.16 | 0.49 | asr_error | ref raise this sentence |
| en-tb064 | rewrite | reply | 0.23 | 0.63 | asr_error | reward this sentence |
| zh-tc089 | rewrite | reply | 0.37 | 0.57 |  | 把这段帮我改成回信的格式 |
| ja-ta017 | rewrite | summarize | 0.05 | 0.91 | self_correction, filler | えーと、要点をまとめて、あ、違う、箇条書きにして |
| ja-ta088 | rewrite | summarize | 0.08 | 0.62 | self_correction | 最初は校正だけって言ったけど、やっぱり内容を整理して、読みやすい箇条書きにして |
| ja-ta149 | rewrite | summarize | 0.00 | 0.99 | asr_error, negation | 翻訳はいらないから、単縮して |
| zh-ta100 | rewrite | summarize | 0.05 | 0.69 | asr_error, self_correction | 概括一下，不对，转成表隔 |
| zh-tb086 | rewrite | summarize | 0.09 | 0.88 | self_correction, filler | 帮我总结一下，嗯不，列成清单就行，每条前面加个序号 |
| zh-tb149 | rewrite | summarize | 0.09 | 0.90 | asr_error | 这页批批踢字太多了，帮我经简一下，只留最关键的几个词 |
| ja-ta042 | rewrite | translate | 0.12 | 0.80 | asr_error | 経語に直して |
| zh-tb059 | rewrite | translate | 0.05 | 0.84 | self_correction | 帮我翻译一下，不是，改得口语化一点，我要发到评论区 |
| zh-tb077 | rewrite | translate | 0.02 | 0.64 | self_correction, filler | 欸，帮我翻译成日文，哦不，换成第一人称来写就行 |
| en-tc111 | summarize | ask | 0.07 | 0.78 | filler, question | I was out sick for two days and came back to eighty messages, um what was this whole Slack thread even about |
| ja-td074 | summarize | ask | 0.04 | 0.95 | asr_error, filler, question | えーと、後方のこの長いお知らせ結局何なの |
| zh-td076 | summarize | ask | 0.46 | 0.53 | asr_error, question | 福建里这份文档主要内容是什么 |
| zh-tc110 | summarize | none | 0.02 | 0.78 | asr_error | 帮我终结一下这条群公告 |
| zh-td011 | summarize | polish | 0.39 | 0.58 | filler, multi_request | 呃你把这段先把错字改了，再给我个摘要 |
| ja-tc116 | summarize | reply | 0.05 | 0.83 | negation, asr_error | まだ返信はいいからさ、とりあえず漸くだけして |
| en-tb101 | summarize | rewrite | 0.12 | 0.86 | multi_request, asr_error | some arise this then make it more formal |
| en-td010 | summarize | rewrite | 0.21 | 0.74 | asr_error, multi_request | my manager needs this for next weak, make it formal and then boil it down to three points |
| ja-ta024 | summarize | rewrite | 0.09 | 0.91 | multi_request | 三行にまとめて、ちょっとカジュアルな言い方にして |
| ja-tb102 | summarize | rewrite | 0.25 | 0.53 | asr_error, self_correction | リライドして、あ、やっぱり要約で |
| ja-tb106 | summarize | rewrite | 0.05 | 0.46 | asr_error, code_mix, multi_request | 用約してtypoも直しておいて |
| ja-tb109 | summarize | rewrite | 0.00 | 1.00 | multi_request | 全体をくだけた口調にして、最後に今日の要点を箇条書きで、ブログの読者向けにね |
| ja-tc119 | summarize | rewrite | 0.15 | 0.33 | asr_error | これ短く予約して |
| ja-td010 | summarize | rewrite | 0.16 | 0.82 | asr_error, multi_request | 車内向けのやつだから丁寧にしてから三つにまとめて |
| ja-td013 | summarize | rewrite | 0.49 | 0.49 | filler, multi_request | あの、ちょっと短くしてから要点出して |
| zh-td013 | summarize | rewrite | 0.18 | 0.81 | multi_request | 把这个，把这段压短一点再总结出重点 |
| ja-ta143 | summarize | translate | 0.00 | 0.99 | asr_error, self_correction | 翻訳して…いや、要訳して |
| ja-tb096 | summarize | translate | 0.23 | 0.74 | asr_error | このスライドの説明、長いから要訳して |
| en-tc134 | translate | reply | 0.21 | 0.75 | asr_error | trains late my reply into Spanish |
| zh-ta098 | translate | rewrite | 0.45 | 0.55 | code_mix, multi_request | 先改得正式一点，再翻成 Japanese |
| en-ta011 | translate | summarize | 0.29 | 0.69 | multi_request | give me a recap and put it in english |
| ja-ta050 | translate | summarize | 0.00 | 0.98 | asr_error | わやくして |
| ja-ta126 | translate | summarize | 0.26 | 0.73 | asr_error, multi_request | 英語にして、それから三行で用約して |
| zh-ta128 | translate | summarize | 0.20 | 0.75 | asr_error, multi_request | 这篇会议记录太长了，你先帮我总结一下重点，然后把总结翻意成英文，发给海外团队，今天下班前要用 |

## qwen3.5-9b rules_kw_fewshot_echo: 69 of 1679 wrong out-of-fold (95.9 accurate)

| gold | predicted | cases |
|---|---|---|
| reply | none | 8 |
| rewrite | polish | 7 |
| polish | rewrite | 5 |
| summarize | rewrite | 5 |
| rewrite | summarize | 5 |
| reply | translate | 3 |
| summarize | ask | 3 |
| summarize | reply | 3 |
| none | reply | 3 |
| summarize | translate | 2 |
| cancel | rewrite | 2 |
| cancel | reply | 2 |
| translate | summarize | 2 |
| reply | polish | 2 |
| ask | none | 2 |
| none | ask | 1 |
| polish | cancel | 1 |
| none | summarize | 1 |
| reply | summarize | 1 |
| translate | reply | 1 |
| translate | rewrite | 1 |
| rewrite | none | 1 |
| translate | polish | 1 |
| polish | translate | 1 |
| none | cancel | 1 |
| ask | reply | 1 |
| polish | reply | 1 |
| cancel | summarize | 1 |
| translate | none | 1 |
| rewrite | reply | 1 |

| tag | cases | wrong | wrong share |
|---|---|---|---|
| asr_error | 542 | 48 | 8.9 |
| self_correction | 181 | 7 | 3.9 |
| retraction | 97 | 3 | 3.1 |
| negation | 116 | 6 | 5.2 |
| filler | 486 | 14 | 2.9 |
| code_mix | 96 | 0 | 0.0 |
| polite | 149 | 2 | 1.3 |
| multi_request | 274 | 14 | 5.1 |
| question | 240 | 8 | 3.3 |

| id | gold | predicted | p(gold) | p(top) | tags | text |
|---|---|---|---|---|---|---|
| ja-tc076 | ask | none | 0.00 | 0.95 | question | この書き方でうちの上司にちゃんと伝わると思う |
| zh-tb016 | ask | none | 0.01 | 0.96 | question | 你是谁啊 |
| ja-tc072 | ask | reply | 0.19 | 0.50 | question | 先方に送ろうと思ってるこの返事、失礼にあたらないかな |
| ja-ta056 | cancel | reply | 0.07 | 0.34 | asr_error | ストップ、注視して |
| zh-tb105 | cancel | reply | 0.03 | 0.76 | asr_error | 布用了 |
| ja-ta040 | cancel | rewrite | 0.33 | 0.59 | self_correction, retraction | 箇条書きにして…今のなしで |
| zh-ta106 | cancel | rewrite | 0.19 | 0.69 | asr_error, retraction, self_correction | 帮我缩短一点，哦不，曲销吧 |
| ja-td088 | cancel | summarize | 0.18 | 0.80 | retraction | これ要約して、あいや短いし自分で読むわ |
| en-ta027 | none | ask | 0.17 | 0.76 | filler | um are you there |
| ja-tb135 | none | cancel | 0.15 | 0.84 | self_correction | 要約して、あ、間違えた、ブラウザでこの言葉を検索して |
| zh-tb116 | none | reply | 0.25 | 0.39 | asr_error | 这个函数的返回值是空字符串，需要在调用前先做盘空，另外异常也要捕获，不然上线会出问题，记得提醒我写测试 |
| zh-tc151 | none | reply | 0.06 | 0.93 | asr_error | 直接把这条回复罚给他 |
| zh-tc156 | none | reply | 0.01 | 0.43 | asr_error | 把群里这条通知制订一下 |
| en-tb141 | none | summarize | 0.11 | 0.87 |  | i think the main argument of the essay is that cities should invest more in public transport because it reduces traffic and it's cheaper than building more roads in the long run |
| en-ta052 | polish | cancel | 0.02 | 0.56 | self_correction | summarize it no no polish it |
| ja-td057 | polish | reply | 0.24 | 0.76 | asr_error | さっきの以来の件で部長への返事なんだけど、変な日本語ないか見て |
| en-ta093 | polish | rewrite | 0.31 | 0.68 | asr_error, negation | no translation needed just make this red more naturally |
| ja-tb018 | polish | rewrite | 0.05 | 0.86 | asr_error | このゼミのレジュメ、ですます調を揃えて、あと五時もよろしく |
| ja-tc132 | polish | rewrite | 0.00 | 0.99 | asr_error | これ構成して |
| zh-tb019 | polish | rewrite | 0.28 | 0.68 | asr_error, filler | 把它理顺一点，呃，对，就是并句 |
| zh-tb138 | polish | rewrite | 0.24 | 0.67 | asr_error | 帮我改错别字，然后给我加上表点 |
| ja-ta147 | polish | translate | 0.13 | 0.75 | asr_error, self_correction, polite | 要約…あ、いや、水耕お願いします |
| en-tc018 | reply | none | 0.22 | 0.70 | asr_error | rely got it I'm setting the staging server up right now |
| en-tc048 | reply | none | 0.11 | 0.88 | filler | uh thank Professor Miller for all the detailed feedback on my draft |
| ja-td036 | reply | none | 0.23 | 0.76 | asr_error | 会議は明日に偏向って知らせて |
| ja-td042 | reply | none | 0.12 | 0.86 | asr_error, filler | あの、承知しました、すぐ太陽しますって |
| zh-tc008 | reply | none | 0.12 | 0.86 | asr_error | 帮我汇付一下底下这留言 |
| zh-tc024 | reply | none | 0.00 | 0.97 | asr_error | 直接帮我汇个可以过去 |
| zh-td030 | reply | none | 0.38 | 0.56 | asr_error, filler | 那个谢谢王老实给的建议 |
| zh-td034 | reply | none | 0.16 | 0.82 | filler | 我今儿发烧起不来床了，嗯说我今天来不了 |
| ja-tc001 | reply | polish | 0.02 | 0.40 | filler, asr_error | えーと、これ変身しといて |
| zh-tc025 | reply | polish | 0.24 | 0.61 | asr_error | 帮我玩具一下今晚这饭局 |
| en-tc024 | reply | summarize | 0.41 | 0.57 | asr_error | write back shore thing |
| en-tc033 | reply | translate | 0.01 | 0.97 | multi_request, asr_error | the climb this sponsorship pitch in Spanish |
| en-td024 | reply | translate | 0.15 | 0.84 | filler, multi_request | um write a reply to this and then translate it into French |
| zh-tc033 | reply | translate | 0.13 | 0.59 | multi_request, asr_error | 帮我用英文把这个商务合作邀约给锯掉，写得礼貌点 |
| ja-ta009 | rewrite | none | 0.02 | 0.90 | asr_error, filler | えーと、かひつして |
| en-tc105 | rewrite | polish | 0.40 | 0.46 | asr_error | titan this paragraph up |
| en-td008 | rewrite | polish | 0.19 | 0.79 | filler, multi_request | uh this is just for our team chat so fix the punctuation and make it more relaxed |
| ja-ta027 | rewrite | polish | 0.02 | 0.95 | asr_error, question, polite | 二区付けしてもらえますか |
| ja-tb064 | rewrite | polish | 0.20 | 0.78 | asr_error, question | マーク ダウンの表にしてくれる？ |
| ja-td004 | rewrite | polish | 0.46 | 0.53 | asr_error, multi_request | ここの暴投のてにをは直してから丁寧な言い方にして |
| zh-ta043 | rewrite | polish | 0.15 | 0.55 | asr_error | 帮我瑞来特一下这句话 |
| zh-ta084 | rewrite | polish | 0.16 | 0.83 | asr_error, negation | 别翻译，帮我改些一下 |
| zh-tc089 | rewrite | reply | 0.08 | 0.69 |  | 把这段帮我改成回信的格式 |
| ja-ta017 | rewrite | summarize | 0.14 | 0.75 | self_correction, filler | えーと、要点をまとめて、あ、違う、箇条書きにして |
| ja-ta088 | rewrite | summarize | 0.18 | 0.81 | self_correction | 最初は校正だけって言ったけど、やっぱり内容を整理して、読みやすい箇条書きにして |
| ja-ta149 | rewrite | summarize | 0.13 | 0.85 | asr_error, negation | 翻訳はいらないから、単縮して |
| ja-tc098 | rewrite | summarize | 0.23 | 0.57 | asr_error | この返信五十時以内に縮めて |
| zh-ta057 | rewrite | summarize | 0.15 | 0.22 | asr_error | 改的正是一点 |
| en-tc111 | summarize | ask | 0.26 | 0.73 | filler, question | I was out sick for two days and came back to eighty messages, um what was this whole Slack thread even about |
| en-td074 | summarize | ask | 0.25 | 0.72 | asr_error, question | management posted this in the haul, what's this long notice even about |
| ja-td074 | summarize | ask | 0.36 | 0.64 | asr_error, filler, question | えーと、後方のこの長いお知らせ結局何なの |
| ja-tc116 | summarize | reply | 0.34 | 0.44 | negation, asr_error | まだ返信はいいからさ、とりあえず漸くだけして |
| zh-tb057 | summarize | reply | 0.03 | 0.47 | asr_error, negation | 不用翻译，也不用润色，帮我写个债要 |
| zh-tc118 | summarize | reply | 0.15 | 0.73 | negation, asr_error | 先别急着回，你帮我书里一下他们吵了半天最后定了啥 |
| en-tb101 | summarize | rewrite | 0.20 | 0.79 | multi_request, asr_error | some arise this then make it more formal |
| ja-ta024 | summarize | rewrite | 0.03 | 0.94 | multi_request | 三行にまとめて、ちょっとカジュアルな言い方にして |
| ja-tb108 | summarize | rewrite | 0.12 | 0.87 | multi_request | ポイントを抜き出して、もうちょっと砕けた口調で |
| ja-tb109 | summarize | rewrite | 0.10 | 0.89 | multi_request | 全体をくだけた口調にして、最後に今日の要点を箇条書きで、ブログの読者向けにね |
| zh-td010 | summarize | rewrite | 0.38 | 0.61 | asr_error, multi_request | 一会儿要往公私大群里丢的，这段改正式一点，再帮我总结成三条 |
| ja-ta001 | summarize | translate | 0.01 | 0.77 | asr_error, multi_request | 箇条書きにして、それをヨーヤクして |
| ja-tb096 | summarize | translate | 0.41 | 0.42 | asr_error | このスライドの説明、長いから要訳して |
| zh-ta128 | translate | none | 0.28 | 0.43 | asr_error, multi_request | 这篇会议记录太长了，你先帮我总结一下重点，然后把总结翻意成英文，发给海外团队，今天下班前要用 |
| ja-ta050 | translate | polish | 0.02 | 0.53 | asr_error | わやくして |
| en-tc134 | translate | reply | 0.23 | 0.76 | asr_error | trains late my reply into Spanish |
| en-tc137 | translate | rewrite | 0.10 | 0.51 | asr_error | put what I wrote here into core ian |
| ja-ta126 | translate | summarize | 0.19 | 0.81 | asr_error, multi_request | 英語にして、それから三行で用約して |
| zh-td015 | translate | summarize | 0.13 | 0.87 | filler, multi_request | 嗯海外团队那边急着看，先总结一下再翻成英文 |
