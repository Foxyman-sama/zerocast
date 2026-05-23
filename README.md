# 1. Переходимо суворо у папку вашого сервера
cd D:\projects\Diploma\zerocast\zerocast_server

# 2. Генеруємо ключі прямо через абсолютний шлях до бінарника Git OpenSSL
& "C:\Program Files\Git\usr\bin\openssl.exe" req -x509 -newkey rsa:2048 -nodes -keyout key.pem -out cert.pem -days 365 -subj "/CN=localhost"

# 3. Пакуємо їх у потрібний .p12 контейнер із паролем 'zerocast'
& "C:\Program Files\Git\usr\bin\openssl.exe" pkcs12 -export -out identity.p12 -inkey key.pem -in cert.pem -password pass:zerocast

# 4. Видаляємо тимчасові текстові залишки ключів
del key.pem, cert.pem